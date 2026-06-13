//! HTTP handlers。/api 给前端与 CI，/internal 给 runner。
//!
//! 安全：/internal 路由用 Bearer token 鉴权（RUNNER_SECRET）；
//! 生产建议在此基础上叠加网络级隔离（VPC/mTLS）。

use std::collections::HashSet;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::stream::Stream;
use serde::Deserialize;
use serde_json::json;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use uuid::Uuid;

use crate::domain::TraceEvent;
use crate::error::{ApiError, ApiResult};
use crate::scheduler;
use crate::stream::Topic;
use crate::AppState;

const MAX_CASES_PER_BATCH: usize = 10_000;
const MAX_PARALLELISM: i32 = 256;
const MAX_PAGE_LIMIT: i64 = 1_000;

type BatchListRow = (
    Uuid,
    String,
    String,
    i64,
    i64,
    f64,
    chrono::DateTime<chrono::Utc>,
);
type RunListRow = (
    Uuid,
    Uuid,
    String,
    String,
    Option<f32>,
    Option<f32>,
    Option<i32>,
);
type RunDetailRow = (
    Uuid,
    String,
    String,
    Option<f32>,
    Option<f32>,
    Option<i32>,
    Option<String>,
    Option<chrono::DateTime<chrono::Utc>>,
    Option<chrono::DateTime<chrono::Utc>>,
);
type CompareRow = (String, String, String, Option<f32>, Option<f32>, Uuid, Uuid);

// ─── 路由注册 ─────────────────────────────────────────────────────────────

pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/profiles", get(list_profiles).post(create_profile))
        .route("/batches", post(create_batch).get(list_batches))
        .route("/batches/:id", get(get_batch))
        .route("/batches/:id/cancel", post(cancel_batch))
        .route("/runs", get(list_runs))
        .route("/runs/:id", get(get_run))
        .route("/runs/:id/trace", get(get_trace))
        .route("/runs/:id/stream", get(stream_run))
        .route("/reports/compare", get(compare_batches))
        .route("/stats/dashboard", get(dashboard_stats))
        .route("/stats/trend", get(pass_rate_trend))
}

pub fn internal_routes() -> Router<AppState> {
    // 鉴权 layer 在 main.rs 用 route_layer(middleware::from_fn_with_state(...)) 注入，
    // 这样 state 在构建时已就绪。
    Router::new()
        .route("/lease", post(lease))
        .route("/runs/:id/heartbeat", post(heartbeat))
        .route("/runs/:id/events", post(ingest_events))
        .route("/runs/:id/complete", post(complete_run))
}

/// GET /metrics — Prometheus text 格式
pub async fn metrics_handler(State(st): State<AppState>) -> String {
    st.prom.render()
}

// ─── 鉴权中间件（由 main.rs 注入到 /internal）──────────────────────────────

pub async fn runner_auth(
    State(st): State<AppState>,
    req: axum::extract::Request,
    next: Next,
) -> Result<axum::response::Response, ApiError> {
    // RUNNER_SECRET 未设置时放行（开发模式，启动时已 warn）
    let Some(secret) = &st.runner_secret else {
        return Ok(next.run(req).await);
    };
    let expected = format!("Bearer {secret}");
    let provided = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if provided != expected {
        return Err(ApiError::Unauthorized);
    }
    Ok(next.run(req).await)
}

// ─── 健康 ────────────────────────────────────────────────────────────────

pub async fn ready(State(st): State<AppState>) -> ApiResult<&'static str> {
    sqlx::query("SELECT 1").execute(&st.db).await?;
    st.traces.health_check().await.map_err(ApiError::Internal)?;
    Ok("ready")
}

// ─── Agent profiles ──────────────────────────────────────────────────────

async fn list_profiles(State(st): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    let rows: Vec<(Uuid, String, String, String, String, String)> = sqlx::query_as(
        "SELECT id, name, scaffold, model, harness_version, sandbox_image
         FROM agent_profiles ORDER BY name",
    )
    .fetch_all(&st.db)
    .await?;
    let items: Vec<_> = rows
        .into_iter()
        .map(|(id, name, scaffold, model, hv, si)| {
            json!({ "id": id, "name": name, "scaffold": scaffold,
                    "model": model, "harness_version": hv, "sandbox_image": si })
        })
        .collect();
    Ok(Json(json!({ "items": items })))
}

#[derive(Deserialize)]
struct CreateProfile {
    name: String,
    scaffold: String,
    model: String,
    harness_version: Option<String>,
    sandbox_image: Option<String>,
    config: Option<serde_json::Value>,
}

async fn create_profile(
    State(st): State<AppState>,
    Json(req): Json<CreateProfile>,
) -> ApiResult<Json<serde_json::Value>> {
    let name = req.name.trim();
    let scaffold = req.scaffold.trim();
    let model = req.model.trim();
    if name.is_empty() {
        return Err(ApiError::BadRequest("name must not be empty".into()));
    }
    if model.is_empty() {
        return Err(ApiError::BadRequest("model must not be empty".into()));
    }
    let valid_scaffolds = [
        "mock",
        "anthropic",
        "langgraph",
        "openai-agents",
        "mini-claude-code",
    ];
    if !valid_scaffolds.contains(&scaffold) {
        return Err(ApiError::BadRequest(format!(
            "scaffold must be one of: {}",
            valid_scaffolds.join(", ")
        )));
    }
    let (id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO agent_profiles (name, scaffold, model, harness_version, sandbox_image, config)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(name)
    .bind(scaffold)
    .bind(model)
    .bind(req.harness_version.as_deref().unwrap_or("dev"))
    .bind(req.sandbox_image.as_deref().unwrap_or("local"))
    .bind(req.config.unwrap_or(json!({})))
    .fetch_one(&st.db)
    .await?;
    Ok(Json(json!({ "id": id })))
}

// ─── 批次 ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CreateBatch {
    name: String,
    profile: String,
    cases: Vec<serde_json::Value>, // [{case_id, task, expectations?}]
    #[serde(default = "default_parallelism")]
    parallelism: i32,
    #[serde(default)]
    priority: i32,
    idempotency_key: Option<String>,
}
fn default_parallelism() -> i32 {
    4
}

async fn create_batch(
    State(st): State<AppState>,
    Json(req): Json<CreateBatch>,
) -> ApiResult<Json<serde_json::Value>> {
    let name = req.name.trim();
    let profile = req.profile.trim();
    if name.is_empty() {
        return Err(ApiError::BadRequest("name must not be empty".into()));
    }
    if profile.is_empty() {
        return Err(ApiError::BadRequest("profile must not be empty".into()));
    }
    if req.cases.is_empty() {
        return Err(ApiError::BadRequest("cases must not be empty".into()));
    }
    if req.cases.len() > MAX_CASES_PER_BATCH {
        return Err(ApiError::BadRequest(format!(
            "cases must not exceed {MAX_CASES_PER_BATCH}"
        )));
    }
    if !(1..=MAX_PARALLELISM).contains(&req.parallelism) {
        return Err(ApiError::BadRequest(format!(
            "parallelism must be between 1 and {MAX_PARALLELISM}"
        )));
    }
    let mut case_ids = HashSet::with_capacity(req.cases.len());
    for c in &req.cases {
        let Some(case_id) = c.get("case_id").and_then(|v| v.as_str()).map(str::trim) else {
            return Err(ApiError::BadRequest(
                "each case needs case_id and task".into(),
            ));
        };
        let Some(task) = c.get("task").and_then(|v| v.as_str()).map(str::trim) else {
            return Err(ApiError::BadRequest(
                "each case needs case_id and task".into(),
            ));
        };
        if case_id.is_empty() || task.is_empty() {
            return Err(ApiError::BadRequest(
                "case_id and task must not be empty".into(),
            ));
        }
        if !case_ids.insert(case_id.to_string()) {
            return Err(ApiError::BadRequest(format!(
                "duplicate case_id in batch: {case_id}"
            )));
        }
    }

    // 幂等：同 key 重复提交返回已有批次（ch49 决策 3）
    if let Some(key) = &req.idempotency_key {
        if let Some((id,)) =
            sqlx::query_as::<_, (Uuid,)>("SELECT id FROM batches WHERE idempotency_key = $1")
                .bind(key)
                .fetch_optional(&st.db)
                .await?
        {
            return Ok(Json(json!({ "id": id, "deduplicated": true })));
        }
    }

    let (profile_id,): (Uuid,) = sqlx::query_as("SELECT id FROM agent_profiles WHERE name = $1")
        .bind(profile)
        .fetch_optional(&st.db)
        .await?
        .ok_or(ApiError::NotFound("profile"))?;

    let mut tx = st.db.begin().await?;
    let (batch_id,): (Uuid,) = sqlx::query_as(
        "INSERT INTO batches (name, profile_id, parallelism, priority, cases, idempotency_key)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(name)
    .bind(profile_id)
    .bind(req.parallelism)
    .bind(req.priority)
    .bind(serde_json::Value::Array(req.cases.clone()))
    .bind(&req.idempotency_key)
    .fetch_one(&mut *tx)
    .await?;

    for c in &req.cases {
        sqlx::query("INSERT INTO runs (batch_id, case_id) VALUES ($1, $2)")
            .bind(batch_id)
            .bind(c["case_id"].as_str().unwrap())
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    tracing::info!(batch_id = %batch_id, name = %name, cases = req.cases.len(), "batch created");
    Ok(Json(json!({ "id": batch_id })))
}

async fn list_batches(State(st): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    let rows: Vec<BatchListRow> = sqlx::query_as(
        r#"SELECT b.id, b.name, b.status,
                      count(r.id) FILTER (WHERE r.status = 'passed'),
                      count(r.id),
                      COALESCE(sum(r.cost_usd)::float8, 0),
                      b.created_at
               FROM batches b LEFT JOIN runs r ON r.batch_id = b.id
               GROUP BY b.id ORDER BY b.created_at DESC LIMIT 100"#,
    )
    .fetch_all(&st.db)
    .await?;
    let items: Vec<_> = rows
        .into_iter()
        .map(|(id, name, status, passed, total, cost, created_at)| {
            json!({
                "id": id, "name": name, "status": status,
                "passed": passed, "total": total,
                "cost_usd": cost,
                "pass_rate": if total > 0 { passed as f64 / total as f64 } else { 0.0 },
                "created_at": created_at
            })
        })
        .collect();
    Ok(Json(json!({ "items": items })))
}

async fn get_batch(
    State(st): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let row: Option<(String, String, i32, chrono::DateTime<chrono::Utc>)> =
        sqlx::query_as("SELECT name, status, parallelism, created_at FROM batches WHERE id = $1")
            .bind(id)
            .fetch_optional(&st.db)
            .await?;
    let (name, status, parallelism, created_at) = row.ok_or(ApiError::NotFound("batch"))?;

    let stats: Vec<(String, i64)> =
        sqlx::query_as("SELECT status, count(*) FROM runs WHERE batch_id = $1 GROUP BY status")
            .bind(id)
            .fetch_all(&st.db)
            .await?;

    Ok(Json(json!({
        "id": id, "name": name, "status": status,
        "parallelism": parallelism, "created_at": created_at,
        "run_stats": stats.into_iter().collect::<std::collections::HashMap<_, _>>(),
    })))
}

async fn cancel_batch(
    State(st): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let mut tx = st.db.begin().await?;
    sqlx::query(
        "UPDATE batches SET status = 'cancelled' WHERE id = $1 AND status IN ('pending','running')",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE runs SET status = 'error', error = 'batch cancelled', finished_at = now()
         WHERE batch_id = $1 AND status = 'queued'",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

// ─── Runs ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct RunFilter {
    batch: Option<Uuid>,
    status: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}
fn default_limit() -> i64 {
    100
}

async fn list_runs(
    State(st): State<AppState>,
    Query(f): Query<RunFilter>,
) -> ApiResult<Json<serde_json::Value>> {
    if !(1..=MAX_PAGE_LIMIT).contains(&f.limit) {
        return Err(ApiError::BadRequest(format!(
            "limit must be between 1 and {MAX_PAGE_LIMIT}"
        )));
    }
    if f.offset < 0 {
        return Err(ApiError::BadRequest("offset must be non-negative".into()));
    }
    if let Some(status) = &f.status {
        if ![
            "queued", "leased", "running", "passed", "failed", "error", "timeout",
        ]
        .contains(&status.as_str())
        {
            return Err(ApiError::BadRequest(format!("invalid status: {status}")));
        }
    }
    let rows: Vec<RunListRow> = sqlx::query_as(
        r#"SELECT id, batch_id, case_id, status, score, cost_usd, turns
               FROM runs
               WHERE ($1::uuid IS NULL OR batch_id = $1)
                 AND ($2::text IS NULL OR status = $2)
               ORDER BY created_at DESC LIMIT $3 OFFSET $4"#,
    )
    .bind(f.batch)
    .bind(&f.status)
    .bind(f.limit)
    .bind(f.offset)
    .fetch_all(&st.db)
    .await?;
    let items: Vec<_> = rows
        .into_iter()
        .map(|(id, batch_id, case_id, status, score, cost, turns)| {
            json!({ "id": id, "batch_id": batch_id, "case_id": case_id,
                    "status": status, "score": score, "cost_usd": cost, "turns": turns })
        })
        .collect();
    Ok(Json(json!({ "items": items })))
}

async fn get_run(
    State(st): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let row: Option<RunDetailRow> = sqlx::query_as(
        "SELECT batch_id, case_id, status, score, cost_usd, turns, error, started_at, finished_at
         FROM runs WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&st.db)
    .await?;
    let (batch_id, case_id, status, score, cost, turns, error, started_at, finished_at) =
        row.ok_or(ApiError::NotFound("run"))?;
    Ok(Json(json!({
        "id": id, "batch_id": batch_id, "case_id": case_id, "status": status,
        "score": score, "cost_usd": cost, "turns": turns, "error": error,
        "started_at": started_at, "finished_at": finished_at
    })))
}

#[derive(Deserialize)]
struct PageQuery {
    #[serde(default)]
    offset: usize,
    #[serde(default = "default_page")]
    limit: usize,
}
fn default_page() -> usize {
    500
}

async fn get_trace(
    State(st): State<AppState>,
    Path(id): Path<Uuid>,
    Query(p): Query<PageQuery>,
) -> ApiResult<Json<crate::store::TracePage>> {
    if p.limit == 0 || p.limit > MAX_PAGE_LIMIT as usize {
        return Err(ApiError::BadRequest(format!(
            "limit must be between 1 and {MAX_PAGE_LIMIT}"
        )));
    }
    Ok(Json(
        st.traces
            .read_page(id, p.offset, p.limit)
            .await
            .map_err(ApiError::Internal)?,
    ))
}

/// SSE：单 run 实时事件流（书第 37 章服务端的生产版）
async fn stream_run(
    State(st): State<AppState>,
    Path(id): Path<Uuid>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = st.hub.subscribe(Topic::Run(id));
    let stream = BroadcastStream::new(rx).filter_map(|msg| match msg {
        Ok(ev) => {
            let data = serde_json::to_string(ev.as_ref()).ok()?;
            Some(Ok(Event::default().id(ev.seq.to_string()).data(data)))
        }
        // 慢消费者掉队：发 lagged 信号，前端走 /trace 全量补偿（ch37）
        Err(BroadcastStreamRecvError::Lagged(n)) => {
            Some(Ok(Event::default().event("lagged").data(n.to_string())))
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}

// ─── 统计 & 仪表盘 ───────────────────────────────────────────────────────

/// GET /api/stats/dashboard — 仪表盘摘要
async fn dashboard_stats(State(st): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    let (total_runs,): (i64,) = sqlx::query_as("SELECT count(*) FROM runs")
        .fetch_one(&st.db)
        .await?;
    let (total_batches,): (i64,) = sqlx::query_as("SELECT count(*) FROM batches")
        .fetch_one(&st.db)
        .await?;
    let (total_cost,): (f64,) =
        sqlx::query_as("SELECT COALESCE(sum(cost_usd)::float8, 0) FROM runs")
            .fetch_one(&st.db)
            .await?;

    let status_counts: Vec<(String, i64)> =
        sqlx::query_as("SELECT status, count(*) FROM runs GROUP BY status")
            .fetch_all(&st.db)
            .await?;
    let passed: i64 = status_counts
        .iter()
        .find(|(s, _)| s == "passed")
        .map(|(_, c)| *c)
        .unwrap_or(0);
    let total_finished: i64 = status_counts
        .iter()
        .filter(|(s, _)| ["passed", "failed", "error", "timeout"].contains(&s.as_str()))
        .map(|(_, c)| *c)
        .sum();
    let pass_rate = if total_finished > 0 {
        passed as f64 / total_finished as f64
    } else {
        0.0
    };

    let (runs_24h,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM runs WHERE created_at > now() - interval '24 hours'")
            .fetch_one(&st.db)
            .await?;

    let (queued,): (i64,) = sqlx::query_as("SELECT count(*) FROM runs WHERE status = 'queued'")
        .fetch_one(&st.db)
        .await?;
    let (active,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM runs WHERE status IN ('leased','running')")
            .fetch_one(&st.db)
            .await?;

    // 更新 Prometheus 队列深度快照
    crate::metrics::update_queue_gauges(queued, active);

    let status_map: std::collections::HashMap<String, i64> = status_counts.into_iter().collect();

    Ok(Json(json!({
        "total_runs": total_runs,
        "total_batches": total_batches,
        "total_cost_usd": total_cost,
        "pass_rate": pass_rate,
        "runs_last_24h": runs_24h,
        "queue_depth": queued,
        "active_runs": active,
        "status_breakdown": status_map
    })))
}

/// GET /api/stats/trend?days=30&group_by=day — 通过率趋势（前端折线图数据源）
#[derive(Deserialize)]
struct TrendQuery {
    #[serde(default = "default_days")]
    days: i32,
    #[serde(default = "default_group_by")]
    group_by: String,
}
fn default_days() -> i32 {
    30
}
fn default_group_by() -> String {
    "day".into()
}

async fn pass_rate_trend(
    State(st): State<AppState>,
    Query(q): Query<TrendQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    if !(1..=365).contains(&q.days) {
        return Err(ApiError::BadRequest(
            "days must be between 1 and 365".into(),
        ));
    }
    // 白名单防 SQL 注入
    let trunc = match q.group_by.as_str() {
        "hour" => "hour",
        "week" => "week",
        _ => "day",
    };
    let sql = format!(
        r#"SELECT date_trunc('{trunc}', finished_at) AS bucket,
                  count(*) FILTER (WHERE status = 'passed') AS passed,
                  count(*) AS total,
                  COALESCE(sum(cost_usd)::float8, 0) AS cost
           FROM runs
           WHERE finished_at > now() - ($1 || ' days')::interval
             AND status IN ('passed', 'failed', 'error', 'timeout')
           GROUP BY bucket
           ORDER BY bucket"#
    );
    let rows: Vec<(chrono::DateTime<chrono::Utc>, i64, i64, f64)> =
        sqlx::query_as(&sql).bind(q.days).fetch_all(&st.db).await?;

    let points: Vec<_> = rows
        .into_iter()
        .map(|(bucket, passed, total, cost)| {
            json!({
                "time": bucket,
                "passed": passed,
                "total": total,
                "cost_usd": cost,
                "pass_rate": if total > 0 { passed as f64 / total as f64 } else { 0.0 }
            })
        })
        .collect();
    Ok(Json(json!({ "points": points })))
}

// ─── 对比报告 ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CompareQuery {
    a: Uuid,
    b: Uuid,
}

async fn compare_batches(
    State(st): State<AppState>,
    Query(q): Query<CompareQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    // 可比性校验（ch49）：profile 指纹不同时附 warning
    let fp: Vec<(Uuid, String, String, String)> = sqlx::query_as(
        "SELECT b.id, p.model, p.harness_version, p.sandbox_image
         FROM batches b JOIN agent_profiles p ON p.id = b.profile_id
         WHERE b.id IN ($1, $2)",
    )
    .bind(q.a)
    .bind(q.b)
    .fetch_all(&st.db)
    .await?;
    if fp.len() != 2 {
        return Err(ApiError::NotFound("batch"));
    }
    let mut warnings: Vec<String> = vec![];
    if fp[0].3 != fp[1].3 {
        warnings.push(format!(
            "sandbox_image 不同（{} vs {}），对比可能无意义",
            fp[0].3, fp[1].3
        ));
    }
    if fp[0].2 != fp[1].2 {
        warnings.push(format!(
            "harness_version 不同（{} vs {}）",
            fp[0].2, fp[1].2
        ));
    }
    if fp[0].1 != fp[1].1 {
        warnings.push(format!("model 不同（{} vs {}）", fp[0].1, fp[1].1));
    }

    let rows: Vec<CompareRow> = sqlx::query_as(
        r#"SELECT a.case_id, a.status, b.status, a.score, b.score, a.id, b.id
               FROM runs a JOIN runs b USING (case_id)
               WHERE a.batch_id = $1 AND b.batch_id = $2
               ORDER BY
                 CASE WHEN a.status = 'passed' AND b.status != 'passed' THEN 0 ELSE 1 END,
                 a.case_id"#,
    )
    .bind(q.a)
    .bind(q.b)
    .fetch_all(&st.db)
    .await?;

    let mut regressions = 0i64;
    let mut improvements = 0i64;
    let cases: Vec<_> = rows
        .into_iter()
        .map(|(case_id, sa, sb, score_a, score_b, run_a, run_b)| {
            let verdict = match (sa.as_str(), sb.as_str()) {
                ("passed", x) if x != "passed" => {
                    regressions += 1;
                    "regression"
                }
                (x, "passed") if x != "passed" => {
                    improvements += 1;
                    "improvement"
                }
                _ => "same",
            };
            json!({
                "case_id": case_id, "status_a": sa, "status_b": sb,
                "score_a": score_a, "score_b": score_b,
                "run_a": run_a, "run_b": run_b, "verdict": verdict
            })
        })
        .collect();

    let total = cases.len() as i64;
    let passed_a: i64 = cases.iter().filter(|c| c["status_a"] == "passed").count() as i64;
    let passed_b: i64 = cases.iter().filter(|c| c["status_b"] == "passed").count() as i64;

    Ok(Json(json!({
        "comparability_warnings": warnings,
        "summary": {
            "total": total,
            "passed_a": passed_a,
            "passed_b": passed_b,
            "pass_rate_a": if total > 0 { passed_a as f64 / total as f64 } else { 0.0 },
            "pass_rate_b": if total > 0 { passed_b as f64 / total as f64 } else { 0.0 },
            "regressions": regressions,
            "improvements": improvements,
        },
        "cases": cases
    })))
}

// ─── Internal（runner 专用）───────────────────────────────────────────────

#[derive(Deserialize)]
struct LeaseReq {
    runner_id: String,
}

async fn lease(
    State(st): State<AppState>,
    Json(req): Json<LeaseReq>,
) -> ApiResult<Json<serde_json::Value>> {
    match scheduler::lease_next(&st.db, &req.runner_id, st.lease_ttl).await? {
        Some(run) => Ok(Json(json!({ "run": run }))),
        None => Ok(Json(json!({ "run": null }))),
    }
}

#[derive(Deserialize)]
struct HeartbeatReq {
    runner_id: String,
}

async fn heartbeat(
    State(st): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<HeartbeatReq>,
) -> ApiResult<Json<serde_json::Value>> {
    let alive = scheduler::heartbeat(&st.db, id, &req.runner_id, st.lease_ttl).await?;
    if !alive {
        tracing::warn!(run_id = %id, runner_id = %req.runner_id, "heartbeat: lease lost");
    }
    Ok(Json(json!({ "alive": alive })))
}

/// runner 批量上报事件（请求体 = JSONL）。双扇出：落盘 + SSE 广播（ch50.3）
async fn ingest_events(
    State(st): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    body: String,
) -> ApiResult<Json<serde_json::Value>> {
    let lines: Vec<&str> = body.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return Ok(Json(json!({ "accepted": 0, "rejected": 0 })));
    }

    let runner_id = headers
        .get("x-runner-id")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| ApiError::BadRequest("X-Runner-Id header is required".into()))?;

    let run_row: Option<(Uuid, Option<String>, String)> =
        sqlx::query_as("SELECT batch_id, runner_id, status FROM runs WHERE id = $1")
            .bind(id)
            .fetch_optional(&st.db)
            .await?;
    let (batch_id, owner, status) = run_row.ok_or(ApiError::NotFound("run"))?;
    if owner.as_deref() != Some(runner_id) {
        return Err(ApiError::Conflict("run not owned by this runner".into()));
    }
    if !["leased", "running"].contains(&status.as_str()) {
        return Err(ApiError::Conflict(format!(
            "run is not accepting events in status {status}"
        )));
    }

    let mut valid: Vec<&str> = Vec::with_capacity(lines.len());
    let mut events: Vec<Arc<TraceEvent>> = Vec::with_capacity(lines.len());
    for line in &lines {
        let ev = serde_json::from_str::<TraceEvent>(line)
            .map_err(|err| ApiError::BadRequest(format!("malformed trace event: {err}")))?;
        ev.validate_for_run(id).map_err(ApiError::BadRequest)?;
        valid.push(line);
        events.push(Arc::new(ev));
    }
    let accepted = valid.len();

    st.traces
        .append(id, &valid)
        .await
        .map_err(ApiError::Internal)?;

    // Prometheus 指标
    crate::metrics::trace_append(accepted);
    crate::metrics::events_ingested(accepted, 0);

    for ev in events {
        st.hub.publish(Topic::Run(id), ev.clone());
        if ev.is_milestone() {
            st.hub.publish(Topic::Batch(batch_id), ev);
        }
    }

    Ok(Json(json!({ "accepted": accepted, "rejected": 0 })))
}

#[derive(Deserialize)]
struct CompleteReq {
    runner_id: String,
    status: String, // passed|failed|error|timeout
    score: Option<f32>,
    cost_usd: Option<f32>,
    turns: Option<i32>,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    error: Option<String>,
    duration_s: Option<f64>, // runner 侧测量的墙钟时间
}

async fn complete_run(
    State(st): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<CompleteReq>,
) -> ApiResult<Json<serde_json::Value>> {
    if !["passed", "failed", "error", "timeout"].contains(&req.status.as_str()) {
        return Err(ApiError::BadRequest(format!(
            "invalid status: {}",
            req.status
        )));
    }
    let trace_path = st.traces.finalize(id);
    // 幂等：只接受属于该 runner 且未完成的 run（ch49 决策 3）
    let n = sqlx::query(
        "UPDATE runs SET status = $2, score = $3, cost_usd = $4, turns = $5,
                input_tokens = $6, output_tokens = $7, error = $8,
                trace_path = $9, finished_at = now()
         WHERE id = $1 AND runner_id = $10 AND status IN ('leased','running')",
    )
    .bind(id)
    .bind(&req.status)
    .bind(req.score)
    .bind(req.cost_usd)
    .bind(req.turns)
    .bind(req.input_tokens)
    .bind(req.output_tokens)
    .bind(&req.error)
    .bind(&trace_path)
    .bind(&req.runner_id)
    .execute(&st.db)
    .await?
    .rows_affected();

    if n == 0 {
        return Err(ApiError::Conflict(
            "run not owned by this runner or already completed".into(),
        ));
    }

    // Prometheus 指标
    crate::metrics::run_completed(&req.status, req.duration_s.unwrap_or(0.0), req.cost_usd);

    st.hub.retire(&Topic::Run(id));
    tracing::info!(run_id = %id, status = %req.status, cost_usd = ?req.cost_usd, "run completed");
    Ok(Json(json!({ "ok": true })))
}
