//! 调度器：任务租约 + reaper（书 50.2 的实现）
//! 核心：`FOR UPDATE SKIP LOCKED` —— Postgres 工作队列的标准姿势，
//! 多 runner 并发领取互不阻塞、绝无双重分配。

use std::time::Duration;

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct LeasedRun {
    pub run_id: Uuid,
    pub batch_id: Uuid,
    pub case_id: String,
    pub task: String,
    pub scaffold: String,
    pub model: String,
    pub expectations: serde_json::Value,
}

/// runner 领任务。返回 None = 当前没有可领的（runner 退避轮询）。
pub async fn lease_next(
    db: &PgPool,
    runner_id: &str,
    ttl: Duration,
) -> sqlx::Result<Option<LeasedRun>> {
    let mut tx = db.begin().await?;

    // 选一条 queued 的 run：批次仍在运行、未超过批次并发上限
    let row: Option<(Uuid, Uuid, String)> = sqlx::query_as(
        r#"
        SELECT r.id, r.batch_id, r.case_id
        FROM runs r
        JOIN batches b ON b.id = r.batch_id
        WHERE r.status = 'queued'
          AND b.status = 'running'
          AND (SELECT count(*) FROM runs x
               WHERE x.batch_id = b.id AND x.status IN ('leased','running')) < b.parallelism
        ORDER BY b.priority DESC, r.created_at
        FOR UPDATE OF r SKIP LOCKED
        LIMIT 1
        "#,
    )
    .fetch_optional(&mut *tx)
    .await?;

    let Some((run_id, batch_id, case_id)) = row else {
        tx.commit().await?;
        return Ok(None);
    };

    sqlx::query(
        "UPDATE runs SET status = 'leased', runner_id = $2,
                         lease_expires_at = now() + $3
         WHERE id = $1",
    )
    .bind(run_id)
    .bind(runner_id)
    .bind(ttl)
    .execute(&mut *tx)
    .await?;

    // 从批次 JSONB 里取该 case 的任务定义 + profile 信息
    let (task, expectations, scaffold, model): (String, serde_json::Value, String, String) =
        sqlx::query_as(
            r#"
            SELECT c->>'task',
                   COALESCE(c->'expectations', '[]'::jsonb),
                   p.scaffold, p.model
            FROM batches b
            JOIN agent_profiles p ON p.id = b.profile_id,
                 LATERAL jsonb_array_elements(b.cases) AS c
            WHERE b.id = $1 AND c->>'case_id' = $2
            "#,
        )
        .bind(batch_id)
        .bind(&case_id)
        .fetch_one(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(Some(LeasedRun {
        run_id,
        batch_id,
        case_id,
        task,
        scaffold,
        model,
        expectations,
    }))
}

/// 心跳续租。返回 false = 租约已被回收（runner 应放弃该任务）。
pub async fn heartbeat(
    db: &PgPool,
    run_id: Uuid,
    runner_id: &str,
    ttl: Duration,
) -> sqlx::Result<bool> {
    let n = sqlx::query(
        "UPDATE runs SET lease_expires_at = now() + $3, status = 'running',
                         started_at = COALESCE(started_at, now())
         WHERE id = $1 AND runner_id = $2 AND status IN ('leased','running')",
    )
    .bind(run_id)
    .bind(runner_id)
    .bind(ttl)
    .execute(db)
    .await?
    .rows_affected();
    Ok(n == 1)
}

/// 集成测试专用：执行一次 reaper 逻辑（不循环）
pub async fn run_reaper_once(db: &PgPool) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE runs SET status = 'queued', runner_id = NULL,
                         lease_expires_at = NULL, retries = retries + 1
         WHERE status IN ('leased','running') AND lease_expires_at < now()
           AND retries < 3",
    )
    .execute(db)
    .await?;
    sqlx::query(
        "UPDATE runs SET status = 'error', error = 'lease expired 3 times', finished_at = now()
         WHERE status IN ('leased','running') AND lease_expires_at < now()",
    )
    .execute(db)
    .await?;
    sqlx::query(
        "UPDATE batches b SET status = 'done'
         WHERE b.status = 'running'
           AND NOT EXISTS (SELECT 1 FROM runs r WHERE r.batch_id = b.id
                           AND r.status IN ('queued','leased','running'))",
    )
    .execute(db)
    .await?;
    Ok(())
}

/// reaper：每 30s 回收过期租约 → 回队重试（上限 3 次后置 error）
pub async fn reaper_loop(db: PgPool) {
    let mut tick = tokio::time::interval(Duration::from_secs(30));
    loop {
        tick.tick().await;
        let requeued = sqlx::query(
            "UPDATE runs SET status = 'queued', runner_id = NULL,
                             lease_expires_at = NULL, retries = retries + 1
             WHERE status IN ('leased','running') AND lease_expires_at < now()
               AND retries < 3",
        )
        .execute(&db)
        .await
        .map(|r| r.rows_affected())
        .unwrap_or(0);

        let dead = sqlx::query(
            "UPDATE runs SET status = 'error', error = 'lease expired 3 times',
                             finished_at = now()
             WHERE status IN ('leased','running') AND lease_expires_at < now()",
        )
        .execute(&db)
        .await
        .map(|r| r.rows_affected())
        .unwrap_or(0);

        if requeued + dead > 0 {
            tracing::warn!(requeued, dead, "reaper reclaimed expired leases");
        }

        // 队列深度快照 → Prometheus
        let queued: i64 = sqlx::query_scalar("SELECT count(*) FROM runs WHERE status = 'queued'")
            .fetch_one(&db)
            .await
            .unwrap_or(0);
        let active: i64 =
            sqlx::query_scalar("SELECT count(*) FROM runs WHERE status IN ('leased','running')")
                .fetch_one(&db)
                .await
                .unwrap_or(0);
        crate::metrics::update_queue_gauges(queued, active);

        // 批次完成检测：没有未完成 run 的 running 批次 → done
        let _ = sqlx::query(
            "UPDATE batches b SET status = 'done'
             WHERE b.status = 'running'
               AND NOT EXISTS (SELECT 1 FROM runs r WHERE r.batch_id = b.id
                               AND r.status IN ('queued','leased','running'))",
        )
        .execute(&db)
        .await;
    }
}
