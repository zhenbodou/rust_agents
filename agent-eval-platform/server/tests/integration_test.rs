//! 集成测试（书第 45 章）：真实 PostgreSQL + MinIO，用 testcontainers 启动。
//!
//! 运行：
//!   cargo test --test integration_test -- --test-threads=1
//!
//! 测试矩阵：
//!   1. 批次创建 + 幂等键
//!   2. 任务调度：lease / heartbeat / complete
//!   3. 事件摄取：JSONL 上报 + 分页读回
//!   4. 租约自愈：reaper 回收过期 lease
//!   5. 对比报告：两批次 A/B 通过率差异

use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt; // oneshot
use uuid::Uuid;

// ─── 测试辅助 ─────────────────────────────────────────────────────────────

/// 启动一个 Postgres 容器并返回连接池
async fn start_postgres() -> (ContainerAsync<Postgres>, sqlx::PgPool) {
    let container = Postgres::default()
        .start()
        .await
        .expect("start postgres container");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("postgres port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .expect("connect to test postgres");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    (container, pool)
}

/// 启动 MinIO 容器，返回 (container, endpoint, bucket)
async fn start_minio() -> (ContainerAsync<GenericImage>, String) {
    let container = GenericImage::new("minio/minio", "RELEASE.2025-09-07T16-13-09Z")
        .with_exposed_port(9000.tcp())
        .with_wait_for(WaitFor::message_on_stderr("API:"))
        .with_env_var("MINIO_ROOT_USER", "minioadmin")
        .with_env_var("MINIO_ROOT_PASSWORD", "minioadmin")
        .with_cmd(["server", "/data"])
        .start()
        .await
        .expect("start minio container");
    let port = container
        .get_host_port_ipv4(9000)
        .await
        .expect("minio port");
    let endpoint = format!("http://127.0.0.1:{port}");
    (container, endpoint)
}

/// 构建被测 Axum App（直接使用连接池，不启动 TCP listener）
async fn build_app(db: sqlx::PgPool, traces: eval_server::store::TraceStore) -> Router {
    use eval_server::{api, metrics, stream::StreamHub, AppState};
    use std::sync::Arc;

    let prom = metrics::install();
    let state = AppState {
        db,
        traces,
        hub: Arc::new(StreamHub::default()),
        lease_ttl: Duration::from_secs(30),
        runner_secret: Some("test-secret".to_string()),
        prom,
    };

    use axum::middleware;
    use axum::routing::get;
    Router::new()
        .nest("/api", api::public_routes())
        .nest(
            "/internal",
            api::internal_routes().route_layer(middleware::from_fn_with_state(
                state.clone(),
                api::runner_auth,
            )),
        )
        .route("/metrics", get(api::metrics_handler))
        .route("/healthz/ready", get(api::ready))
        .with_state(state)
}

/// 发送 JSON 请求，返回状态码 + 响应 JSON
async fn req(
    app: &Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
    auth: bool,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if auth {
        builder = builder.header("Authorization", "Bearer test-secret");
    }
    let body = match body {
        Some(v) => {
            let bytes = serde_json::to_vec(&v).unwrap();
            builder = builder.header("Content-Type", "application/json");
            Body::from(bytes)
        }
        None => Body::empty(),
    };
    let req = builder.body(body).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

// ─── 测试 1：批次创建 + 幂等键 ───────────────────────────────────────────

#[tokio::test]
async fn test_create_batch_idempotent() {
    let (_pg, db) = start_postgres().await;
    let traces = eval_server::store::TraceStore::new_local_tmp();
    let app = build_app(db.clone(), traces).await;

    // 先创建 profile
    let (status, _) = req(
        &app,
        Method::POST,
        "/api/profiles",
        Some(json!({
            "name": "test-agent",
            "scaffold": "mock",
            "model": "claude-3-haiku-20240307"
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let batch_body = json!({
        "name": "smoke-test",
        "profile": "test-agent",
        "idempotency_key": "test-key-001",
        "cases": [
            { "case_id": "c1", "task": "echo hello" },
            { "case_id": "c2", "task": "echo world" },
        ]
    });

    // 第一次创建
    let (status, resp1) = req(
        &app,
        Method::POST,
        "/api/batches",
        Some(batch_body.clone()),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "first create failed: {resp1}");
    let id1 = resp1["id"].as_str().unwrap().to_string();

    // 同 idempotency_key 再次创建 → 应返回同一 id
    let (status, resp2) = req(&app, Method::POST, "/api/batches", Some(batch_body), false).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resp2["id"].as_str().unwrap(), id1, "idempotency broken");
    assert_eq!(resp2["deduplicated"].as_bool(), Some(true));
}

// ─── 测试 2：任务调度完整流程 ─────────────────────────────────────────────

#[tokio::test]
async fn test_lease_heartbeat_complete() {
    let (_pg, db) = start_postgres().await;
    let traces = eval_server::store::TraceStore::new_local_tmp();
    let app = build_app(db.clone(), traces).await;

    // 创建 profile + batch
    req(
        &app,
        Method::POST,
        "/api/profiles",
        Some(json!({"name":"ag","scaffold":"mock","model":"m"})),
        false,
    )
    .await;
    let batch_body = json!({
        "name": "lease-test", "profile": "ag",
        "cases": [{"case_id":"t1","task":"do it"}]
    });
    let (_, br) = req(&app, Method::POST, "/api/batches", Some(batch_body), false).await;
    let _batch_id = br["id"].as_str().unwrap();

    // 无 auth → 401
    let (status, _) = req(
        &app,
        Method::POST,
        "/internal/lease",
        Some(json!({"runner_id":"r1"})),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // 有 auth → 拿到任务
    let (status, lr) = req(
        &app,
        Method::POST,
        "/internal/lease",
        Some(json!({"runner_id":"r1"})),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "lease failed: {lr}");
    let run = &lr["run"];
    assert!(!run.is_null(), "expected a run, got null");
    let run_id = run["run_id"].as_str().unwrap().to_string();

    // heartbeat
    let (status, hr) = req(
        &app,
        Method::POST,
        &format!("/internal/runs/{run_id}/heartbeat"),
        Some(json!({"runner_id":"r1"})),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(hr["alive"].as_bool(), Some(true));

    // 上报事件
    let event_line = serde_json::to_string(&json!({
        "schema_version": 1, "seq": 0, "ts": 1234567890.0,
        "type": "run_started", "run_id": run_id,
        "case_id": "t1", "task": "do it", "model": "m"
    }))
    .unwrap();
    let req_obj = Request::builder()
        .method(Method::POST)
        .uri(format!("/internal/runs/{run_id}/events"))
        .header("Authorization", "Bearer test-secret")
        .header("X-Runner-Id", "r1")
        .header("Content-Type", "text/plain")
        .body(Body::from(event_line))
        .unwrap();
    let resp = app.clone().oneshot(req_obj).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // complete
    let (status, cr) = req(
        &app,
        Method::POST,
        &format!("/internal/runs/{run_id}/complete"),
        Some(json!({"runner_id":"r1","status":"passed","score":0.9,"turns":3,"duration_s":1.2})),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "complete failed: {cr}");

    // 验证 run 状态
    let (status, run_resp) = req(
        &app,
        Method::GET,
        &format!("/api/runs/{run_id}"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(run_resp["status"].as_str(), Some("passed"));
    assert!((run_resp["score"].as_f64().unwrap() - 0.9).abs() < 0.001);
}

// ─── 测试 3：事件摄取 + 分页读回 ─────────────────────────────────────────

#[tokio::test]
async fn test_event_ingest_and_trace_page() {
    let (_pg, db) = start_postgres().await;
    let traces = eval_server::store::TraceStore::new_local_tmp();
    let app = build_app(db.clone(), traces).await;

    req(
        &app,
        Method::POST,
        "/api/profiles",
        Some(json!({"name":"ag","scaffold":"mock","model":"m"})),
        false,
    )
    .await;
    let (_, br) = req(
        &app,
        Method::POST,
        "/api/batches",
        Some(json!({"name":"t","profile":"ag","cases":[{"case_id":"c","task":"x"}]})),
        false,
    )
    .await;
    let _batch_id = br["id"].as_str().unwrap();

    let (_, lr) = req(
        &app,
        Method::POST,
        "/internal/lease",
        Some(json!({"runner_id":"r1"})),
        true,
    )
    .await;
    let run_id = lr["run"]["run_id"].as_str().unwrap().to_string();

    // 构建 20 个事件行
    let lines: String = (0..20)
        .map(|i| {
            serde_json::to_string(&json!({
                "schema_version": 1, "seq": i, "ts": 1_700_000_000.0 + i as f64,
                "type": "llm_chunk", "turn": 0, "delta": format!("token_{i}")
            }))
            .unwrap()
        })
        .collect::<Vec<_>>()
        .join("\n");

    let req_obj = Request::builder()
        .method(Method::POST)
        .uri(format!("/internal/runs/{run_id}/events"))
        .header("Authorization", "Bearer test-secret")
        .header("X-Runner-Id", "r1")
        .body(Body::from(lines))
        .unwrap();
    let resp = app.clone().oneshot(req_obj).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 第 1 页（limit=10）
    let (status, page1) = req(
        &app,
        Method::GET,
        &format!("/api/runs/{run_id}/trace?offset=0&limit=10"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page1["events"].as_array().unwrap().len(), 10);
    assert_eq!(page1["total"].as_u64(), Some(20));
    assert_eq!(page1["next_offset"].as_u64(), Some(10));

    // 第 2 页
    let (_, page2) = req(
        &app,
        Method::GET,
        &format!("/api/runs/{run_id}/trace?offset=10&limit=10"),
        None,
        false,
    )
    .await;
    assert_eq!(page2["next_offset"], Value::Null);
    assert_eq!(
        page2["events"].as_array().unwrap()[0]["seq"].as_u64(),
        Some(10)
    );
}

// ─── 测试 3b：事件与批次输入安全校验 ────────────────────────────────────

#[tokio::test]
async fn test_event_security_validation() {
    let (_pg, db) = start_postgres().await;
    let traces = eval_server::store::TraceStore::new_local_tmp();
    let app = build_app(db.clone(), traces).await;

    req(
        &app,
        Method::POST,
        "/api/profiles",
        Some(json!({"name":"ag","scaffold":"mock","model":"m"})),
        false,
    )
    .await;
    req(
        &app,
        Method::POST,
        "/api/batches",
        Some(json!({"name":"t","profile":"ag","cases":[{"case_id":"c","task":"x"}]})),
        false,
    )
    .await;

    let (_, lr) = req(
        &app,
        Method::POST,
        "/internal/lease",
        Some(json!({"runner_id":"owner-runner"})),
        true,
    )
    .await;
    let run_id = lr["run"]["run_id"].as_str().unwrap().to_string();

    let valid_event = serde_json::to_string(&json!({
        "schema_version": 1, "seq": 0, "ts": 1_700_000_000.0,
        "type": "run_started", "run_id": run_id,
        "case_id": "c", "task": "x", "model": "m"
    }))
    .unwrap();

    let wrong_runner_req = Request::builder()
        .method(Method::POST)
        .uri(format!("/internal/runs/{run_id}/events"))
        .header("Authorization", "Bearer test-secret")
        .header("X-Runner-Id", "other-runner")
        .body(Body::from(valid_event.clone()))
        .unwrap();
    let wrong_runner_resp = app.clone().oneshot(wrong_runner_req).await.unwrap();
    assert_eq!(wrong_runner_resp.status(), StatusCode::CONFLICT);

    let bad_schema = valid_event.replace("\"schema_version\":1", "\"schema_version\":2");
    let bad_schema_req = Request::builder()
        .method(Method::POST)
        .uri(format!("/internal/runs/{run_id}/events"))
        .header("Authorization", "Bearer test-secret")
        .header("X-Runner-Id", "owner-runner")
        .body(Body::from(bad_schema))
        .unwrap();
    let bad_schema_resp = app.clone().oneshot(bad_schema_req).await.unwrap();
    assert_eq!(bad_schema_resp.status(), StatusCode::BAD_REQUEST);

    let mismatched_run_id = Uuid::new_v4();
    let mismatched_event = serde_json::to_string(&json!({
        "schema_version": 1, "seq": 0, "ts": 1_700_000_000.0,
        "type": "run_started", "run_id": mismatched_run_id,
        "case_id": "c", "task": "x", "model": "m"
    }))
    .unwrap();
    let mismatched_req = Request::builder()
        .method(Method::POST)
        .uri(format!("/internal/runs/{run_id}/events"))
        .header("Authorization", "Bearer test-secret")
        .header("X-Runner-Id", "owner-runner")
        .body(Body::from(mismatched_event))
        .unwrap();
    let mismatched_resp = app.clone().oneshot(mismatched_req).await.unwrap();
    assert_eq!(mismatched_resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_batch_input_validation() {
    let (_pg, db) = start_postgres().await;
    let traces = eval_server::store::TraceStore::new_local_tmp();
    let app = build_app(db.clone(), traces).await;

    req(
        &app,
        Method::POST,
        "/api/profiles",
        Some(json!({"name":"ag","scaffold":"mock","model":"m"})),
        false,
    )
    .await;

    let (status, _) = req(
        &app,
        Method::POST,
        "/api/batches",
        Some(json!({
            "name": "bad-parallelism", "profile": "ag", "parallelism": 0,
            "cases": [{"case_id":"c1","task":"x"}]
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = req(
        &app,
        Method::POST,
        "/api/batches",
        Some(json!({
            "name": "duplicate-cases", "profile": "ag",
            "cases": [
                {"case_id":"same","task":"x"},
                {"case_id":"same","task":"y"}
            ]
        })),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ─── 测试 4：租约自愈（reaper）────────────────────────────────────────────

#[tokio::test]
async fn test_reaper_reclaims_expired_lease() {
    let (_pg, db) = start_postgres().await;
    let traces = eval_server::store::TraceStore::new_local_tmp();
    let app = build_app(db.clone(), traces).await;

    req(
        &app,
        Method::POST,
        "/api/profiles",
        Some(json!({"name":"ag","scaffold":"mock","model":"m"})),
        false,
    )
    .await;
    let (_, br) = req(
        &app,
        Method::POST,
        "/api/batches",
        Some(json!({"name":"reap-test","profile":"ag","cases":[{"case_id":"r","task":"t"}]})),
        false,
    )
    .await;
    let _batch_id = br["id"].as_str().unwrap().to_string();

    // lease（TTL 超短，直接 SQL 设置过去时间来模拟过期）
    let (_, lr) = req(
        &app,
        Method::POST,
        "/internal/lease",
        Some(json!({"runner_id":"dead-runner"})),
        true,
    )
    .await;
    let run_id: Uuid = lr["run"]["run_id"].as_str().unwrap().parse().unwrap();

    // 强制让租约立即过期
    sqlx::query("UPDATE runs SET lease_expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(run_id)
        .execute(&db)
        .await
        .unwrap();

    // 手动调用 reaper 一次
    eval_server::scheduler::run_reaper_once(&db).await.unwrap();

    // run 应该回到 queued
    let (status_str,): (String,) = sqlx::query_as("SELECT status FROM runs WHERE id = $1")
        .bind(run_id)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(status_str, "queued", "reaper should have reset to queued");

    // 另一个 runner 应该能领到这个任务
    let (_, lr2) = req(
        &app,
        Method::POST,
        "/internal/lease",
        Some(json!({"runner_id":"healthy-runner"})),
        true,
    )
    .await;
    assert!(!lr2["run"].is_null(), "healthy runner should get the task");
}

// ─── 测试 5：对比报告 ─────────────────────────────────────────────────────

#[tokio::test]
async fn test_compare_report() {
    let (_pg, db) = start_postgres().await;
    let traces = eval_server::store::TraceStore::new_local_tmp();
    let app = build_app(db.clone(), traces).await;

    req(
        &app,
        Method::POST,
        "/api/profiles",
        Some(json!({"name":"ag","scaffold":"mock","model":"m"})),
        false,
    )
    .await;

    // 批次 A
    let (_, ba) = req(
        &app,
        Method::POST,
        "/api/batches",
        Some(json!({
            "name": "batch-a", "profile": "ag",
            "cases": [
                {"case_id":"c1","task":"t1"},
                {"case_id":"c2","task":"t2"},
                {"case_id":"c3","task":"t3"},
            ]
        })),
        false,
    )
    .await;
    let bid_a = ba["id"].as_str().unwrap().to_string();

    // 批次 B
    let (_, bb) = req(
        &app,
        Method::POST,
        "/api/batches",
        Some(json!({
            "name": "batch-b", "profile": "ag",
            "cases": [
                {"case_id":"c1","task":"t1"},
                {"case_id":"c2","task":"t2"},
                {"case_id":"c3","task":"t3"},
            ]
        })),
        false,
    )
    .await;
    let bid_b = bb["id"].as_str().unwrap().to_string();

    // 手动设置 run 状态（绕过 runner，直接 SQL）
    // A: c1=passed, c2=passed, c3=failed
    // B: c1=passed, c2=failed, c3=passed
    sqlx::query(
        "UPDATE runs r SET status = v.status, finished_at = now()
         FROM (VALUES ('c1','passed'),('c2','passed'),('c3','failed')) AS v(case_id, status)
         WHERE r.batch_id = $1 AND r.case_id = v.case_id",
    )
    .bind(bid_a.parse::<Uuid>().unwrap())
    .execute(&db)
    .await
    .unwrap();

    sqlx::query(
        "UPDATE runs r SET status = v.status, finished_at = now()
         FROM (VALUES ('c1','passed'),('c2','failed'),('c3','passed')) AS v(case_id, status)
         WHERE r.batch_id = $1 AND r.case_id = v.case_id",
    )
    .bind(bid_b.parse::<Uuid>().unwrap())
    .execute(&db)
    .await
    .unwrap();

    // 调用对比接口
    let (status, report) = req(
        &app,
        Method::GET,
        &format!("/api/reports/compare?a={bid_a}&b={bid_b}"),
        None,
        false,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "compare failed: {report}");

    let summary = &report["summary"];
    assert_eq!(summary["total"].as_u64(), Some(3));
    assert_eq!(summary["passed_a"].as_u64(), Some(2));
    assert_eq!(summary["passed_b"].as_u64(), Some(2));
    assert_eq!(
        summary["regressions"].as_u64(),
        Some(1),
        "c2 should be regression"
    );
    assert_eq!(
        summary["improvements"].as_u64(),
        Some(1),
        "c3 should be improvement"
    );

    // c2: A=passed, B=failed → regression
    let cases = report["cases"].as_array().unwrap();
    let c2 = cases.iter().find(|c| c["case_id"] == "c2").unwrap();
    assert_eq!(c2["verdict"].as_str(), Some("regression"));

    // c3: A=failed, B=passed → improvement
    let c3 = cases.iter().find(|c| c["case_id"] == "c3").unwrap();
    assert_eq!(c3["verdict"].as_str(), Some("improvement"));
}

// ─── 测试 6：仪表盘统计 ───────────────────────────────────────────────────

#[tokio::test]
async fn test_dashboard_stats() {
    let (_pg, db) = start_postgres().await;
    let traces = eval_server::store::TraceStore::new_local_tmp();
    let app = build_app(db.clone(), traces).await;

    let (status, dash) = req(&app, Method::GET, "/api/stats/dashboard", None, false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(dash["total_runs"].is_number());
    assert!(dash["pass_rate"].is_number());
}

// ─── 测试 7：MinIO 后端健康检查 ──────────────────────────────────────────

#[tokio::test]
async fn test_minio_health_check() {
    let (_pg, _db) = start_postgres().await;
    let (_minio, endpoint) = start_minio().await;

    // 创建 bucket（MinIO 需要提前建）
    let minio_client = reqwest::Client::new();
    minio_client
        .put(format!("{endpoint}/traces"))
        .header("Authorization", "AWS minioadmin:minioadmin")
        .send()
        .await
        .ok(); // 简化：只确保服务可达

    // 用 S3 后端构建 TraceStore
    std::env::set_var("TRACE_BACKEND", "s3");
    std::env::set_var("S3_ENDPOINT", &endpoint);
    std::env::set_var("S3_BUCKET", "traces");
    std::env::set_var("S3_ACCESS_KEY", "minioadmin");
    std::env::set_var("S3_SECRET_KEY", "minioadmin");

    // 由于 MinIO 需要 bucket 存在，这里主要验证 from_env() 不 panic
    let traces = eval_server::store::TraceStore::from_env();
    assert!(
        traces.is_ok(),
        "S3 TraceStore failed to build: {:?}",
        traces.err()
    );

    // 清理环境变量
    std::env::remove_var("TRACE_BACKEND");
}
