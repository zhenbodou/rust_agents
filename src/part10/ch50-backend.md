# 第 50 章 后端实现：任务调度、轨迹存储与实时广播

> 本章实现后端的三个核心：怎么把任务公平分给一堆 Runner、怎么用 `object_store` crate 把巨大的轨迹写进 S3/MinIO、怎么把事件实时广播给所有正在看的人。还覆盖了生产必须的两件事：Prometheus 指标暴露和内部路由鉴权。完整代码在 `agent-eval-platform/server/`。

## 50.1 模块地图

后端分六个模块，一行说清楚各自的职责：

```
server/src/
├── lib.rs       ← 库入口（供集成测试 import）
├── main.rs      ← 二进制入口：配置、连库、组装路由、启动
├── domain.rs    ← 领域类型：TraceEvent 判别联合体
├── store.rs     ← 对象存储门面：local ↔ S3/MinIO，接口统一
├── stream.rs    ← SSE 广播中心：按 topic 分发
├── scheduler.rs ← 任务租约 + reaper 自愈
├── api.rs       ← REST + SSE handlers
├── metrics.rs   ← Prometheus 指标定义
└── error.rs     ← 统一错误类型 → HTTP 响应
```

**把服务做成"库 + 二进制"双入口**是个生产级习惯——`lib.rs` 导出所有公共模块，集成测试直接 `use eval_server::scheduler::run_reaper_once`，不需要 HTTP 调用绕一圈（第 45 章的道理在这里兑现）。

## 50.2 调度器：`FOR UPDATE SKIP LOCKED` 的生产级用法

核心问题：多个 Runner 同时来抢任务，怎么保证**一个任务只被一个 Runner 领走**？

答案是 PostgreSQL 的 `FOR UPDATE SKIP LOCKED`——它让每个事务只锁自己拿到的行，拿不到就**静默跳过**（不等不阻塞），天然支持高并发领取：

```rust
// scheduler.rs — lease_next()：在事务里 SKIP LOCKED 领任务
let row: Option<(Uuid, Uuid, String)> = sqlx::query_as(
    r#"
    SELECT r.id, r.batch_id, r.case_id
    FROM runs r
    JOIN batches b ON b.id = r.batch_id
    WHERE r.status = 'queued'
      AND b.status = 'running'
      AND (SELECT count(*) FROM runs x
           WHERE x.batch_id = b.id
             AND x.status IN ('leased','running')) < b.parallelism
    ORDER BY b.priority DESC, r.created_at
    FOR UPDATE OF r SKIP LOCKED
    LIMIT 1
    "#,
)
.fetch_optional(&mut *tx)
.await?;
```

三个值得注意的点：

1. **`SKIP LOCKED` 而非 `NOWAIT`**：`NOWAIT` 拿不到锁就报错，`SKIP LOCKED` 拿不到就跳下一个，后者才是队列正确的打开方式。
2. **并发上限检查在 SQL 里**：子查询 `count(*) < b.parallelism` 让批次并发度控制也在一个原子事务里完成，不会超发。
3. **优先级列 `b.priority`**：生产里不同批次的紧急程度不同，加一个 `priority` 列、`ORDER BY priority DESC` 就能实现不同批次的调度优先级。

状态机如下：

```
queued ──lease──► leased ──heartbeat──► running ──complete──► passed/failed/error
  ▲                  │                    │
  └── reaper 回收 ───┴────────────────────┘（超过 3 次 → error）
```

Reaper 每 30 秒跑一次，同时把队列深度快照推给 Prometheus：

```rust
// scheduler.rs — reaper_loop()
let queued: i64 = sqlx::query_scalar("SELECT count(*) FROM runs WHERE status = 'queued'")
    .fetch_one(&db).await.unwrap_or(0);
let active: i64 = sqlx::query_scalar(
    "SELECT count(*) FROM runs WHERE status IN ('leased','running')"
).fetch_one(&db).await.unwrap_or(0);
crate::metrics::update_queue_gauges(queued, active);
```

这样 Grafana 就能实时看到队列积压了多少——这在批量评测的生产环境里是最重要的运维指标之一。

## 50.3 轨迹存储：`object_store` 统一抽象

原本设计用本地文件系统。生产问题：**多节点部署时，文件在哪台机器？**对象存储（S3/MinIO）是标准答案，但切换后端不该改接口。Rust 生态的 `object_store` crate（Apache Arrow 项目，Apache/DataFusion 都在用）提供统一的 `Arc<dyn ObjectStore>` 抽象：

```rust
// store.rs — TraceStore 门面
#[derive(Clone)]
pub struct TraceStore {
    inner: Arc<dyn ObjectStore>,  // ← 同一个接口，本地 / S3 / GCS 随便换
    prefix: String,
}

impl TraceStore {
    pub fn from_env() -> Result<Self> {
        match std::env::var("TRACE_BACKEND").as_deref() {
            Ok("s3") | Ok("minio") => Self::new_s3(),
            _ => Self::new_local(),
        }
    }

    fn new_s3() -> Result<Self> {
        let store = AmazonS3Builder::new()
            .with_endpoint(std::env::var("S3_ENDPOINT")?)
            .with_bucket_name(std::env::var("S3_BUCKET")?)
            .with_access_key_id(std::env::var("S3_ACCESS_KEY")?)
            .with_secret_access_key(std::env::var("S3_SECRET_KEY")?)
            // MinIO 用路径寻址，不用虚拟主机寻址
            .with_virtual_hosted_style_request(false)
            .build()?;
        Ok(Self { inner: Arc::new(store), prefix: String::new() })
    }
}
```

**写入策略（教学版 trade-off）**：S3 不支持原生 append，用"读旧 + 追加 + 写回"：

```rust
pub async fn append(&self, run_id: Uuid, lines: &[&str]) -> Result<()> {
    let key = self.key(run_id);
    // 读现有内容（首次写不存在是正常情况）
    let existing = match self.inner.get(&key).await {
        Ok(r)  => r.bytes().await?.to_vec(),
        Err(object_store::Error::NotFound { .. }) => vec![],
        Err(e) => return Err(e.into()),
    };
    let mut merged = existing;
    for l in lines { merged.extend_from_slice(l.as_bytes()); merged.push(b'\n'); }
    self.inner.put_opts(&key, merged.into(), PutOptions { mode: PutMode::Overwrite, ..Default::default() }).await?;
    Ok(())
}
```

> **生产优化**：高频上报（比如每秒 100 个 token）会产生写放大。生产版改成"分段积累 → 每 N 秒上传一个 part → 完成时 multipart complete 合并"。教学版刻意保持接口简单，trade-off 已在注释里写明，面试时要能讲出来。

健康检查也用 `object_store`，写一个哨兵对象验证连通性，`/healthz/ready` 同时检查 Postgres 和对象存储：

```rust
pub async fn health_check(&self) -> Result<()> {
    let key = ObjPath::from("_healthcheck");
    self.inner.put(&key, PutPayload::from_static(b"ok")).await?;
    let _ = self.inner.delete(&key).await;
    Ok(())
}
```

## 50.4 Prometheus 指标：内置可观测性

第 15 章讲的可观测性理论，这里落地。用 `metrics` + `metrics-exporter-prometheus` crate：

```rust
// metrics.rs — 指标定义（在函数里按需调用，无需全局注册）
pub fn run_completed(status: &str, duration_secs: f64, cost_usd: Option<f32>) {
    counter!("eval_runs_total", "status" => status.to_string()).increment(1);
    histogram!("eval_run_duration_seconds", "status" => status.to_string()).record(duration_secs);
    if let Some(cost) = cost_usd {
        histogram!("eval_run_cost_usd", "status" => status.to_string()).record(cost as f64);
    }
}

pub fn update_queue_gauges(queued: i64, active: i64) {
    gauge!("eval_queue_depth").set(queued as f64);
    gauge!("eval_active_runs").set(active as f64);
}
```

`GET /metrics` 暴露 Prometheus text 格式，Prometheus 每 15 秒来 scrape 一次。指标分三层：

| 层次 | 指标 | 用来回答什么问题 |
|---|---|---|
| 请求层 | `http_requests_total{status}` | 接口成功率 / 错误分布 |
| 业务层 | `eval_runs_total{status}` | 通过率趋势（这才是平台的核心 KPI）|
| 队列层 | `eval_queue_depth` / `eval_active_runs` | 积压了多少，扩不扩 Runner |

## 50.5 内部路由鉴权：Bearer token 中间件

`/internal` 接口只给 Runner 调用，不应该对外开放。生产最低线：**Bearer token 验证**。用 axum 的 `from_fn_with_state` middleware 注入到路由层：

```rust
// api.rs — 鉴权中间件
pub async fn runner_auth(
    State(st): State<AppState>,
    req: axum::extract::Request,
    next: Next,
) -> Result<Response, ApiError> {
    let Some(secret) = &st.runner_secret else {
        return Ok(next.run(req).await);  // RUNNER_SECRET 未设置 → 开发放行
    };
    let provided = req.headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if provided != format!("Bearer {secret}") {
        return Err(ApiError::Unauthorized);
    }
    Ok(next.run(req).await)
}

// main.rs — 把 middleware 挂到 /internal 路由组
.nest(
    "/internal",
    api::internal_routes().route_layer(middleware::from_fn_with_state(
        state.clone(), api::runner_auth,
    )),
)
```

**注意两个生产建议**（面试中要能讲出来）：

1. **Token 不够，还要网络隔离**：生产里 `/internal` 路由应该只在集群内网可达（通过 K8s NetworkPolicy），Bearer token 只是第二道防线。
2. **开发模式显式 warn**：`RUNNER_SECRET` 未设置时不是静默放行，而是启动时打 `warn!`，避免测试时忘配 + 上生产没发现。

## 50.6 仪表盘 SQL：统计查询与趋势聚合

`/api/stats/dashboard` 和 `/api/stats/trend` 是前端仪表盘的数据源，核心在 SQL 聚合。趋势接口用 `date_trunc` 按天/小时/周分桶：

```sql
-- /api/stats/trend?days=30&group_by=day
SELECT date_trunc('day', finished_at) AS bucket,
       count(*) FILTER (WHERE status = 'passed') AS passed,
       count(*) AS total,
       COALESCE(sum(cost_usd)::float8, 0) AS cost
FROM runs
WHERE finished_at > now() - ($1 || ' days')::interval
  AND status IN ('passed', 'failed', 'error', 'timeout')
GROUP BY bucket
ORDER BY bucket
```

`group_by` 参数用白名单硬限制（`"hour" | "week" | "day"`），不拼进 SQL 变量——这是防 SQL 注入的基本操作，防的是有人传 `'; DROP TABLE runs; --`。

## 50.7 集成测试：真容器，不 mock

用 `testcontainers-rs` 在 CI 里起真实 Postgres + MinIO：

```rust
// tests/integration_test.rs
#[tokio::test]
async fn test_lease_heartbeat_complete() {
    let (_pg, db) = start_postgres().await;   // 起真实 Postgres 容器
    let traces = TraceStore::new_local_tmp(); // 临时目录
    let app = build_app(db, traces).await;

    // 无 auth → 401
    let (status, _) = req(&app, POST, "/internal/lease",
        Some(json!({"runner_id":"r1"})), false).await;
    assert_eq!(status, 401);

    // 有 auth → 拿到任务 → heartbeat → complete
    let (_, lr) = req(&app, POST, "/internal/lease",
        Some(json!({"runner_id":"r1"})), true).await;
    let run_id = lr["run"]["run_id"].as_str().unwrap();
    // ... heartbeat, complete, 验证状态 ...
}
```

测试策略：

- 调度并发正确性（一个任务只分一个 runner）→ 多并发协程同时 lease，验证 run_id 无重复
- reaper 自愈 → SQL 强制过期 lease，调用 `run_reaper_once()`，验证状态回到 queued
- 对比报告 → 两个批次 SQL 直填状态，验证 regression/improvement 计数
- MinIO 联通性 → 起真实 MinIO 容器，验证 `from_env()` 不 panic、`health_check()` 成功

## 50.8 小结与练习

- `FOR UPDATE SKIP LOCKED` + 状态机 + reaper = 分布式任务队列，自愈，不依赖外部 MQ。
- `object_store` crate 让本地 / S3 / GCS 无缝切换，代码一行不改。
- Prometheus 指标三层：请求 / 业务 / 队列——缺一不可。
- Bearer token + `from_fn_with_state` 是 axum 鉴权 middleware 的标准模式。
- 集成测试用真容器，否则测不出 `SKIP LOCKED` 和 MinIO 协议的真实行为。

**练习**

1. 把租约 TTL 改成 3 秒，启动服务，用 `wrk` 压 `/internal/lease` 看 `eval_queue_depth` 指标是否实时下降。
2. 实现"优先级批次插队"：批次表加 `priority INT DEFAULT 0`（已有），前端加"紧急"开关，写一个测试验证高优先级批次先被领取。
3. 对 `append()` 做性能测试：对同一 run 连续调用 1000 次，观察 S3 请求数随 batch size 的变化，找出最优批量大小。

> **下一章**：前端实现——把轨迹查看器、仪表盘趋势图、对比报告页面组装起来，解决"实时数据和历史数据怎么无缝拼接"这类组装时才会遇到的真问题。
