//! Prometheus 指标定义与注册（书第 15 章生产级可观测性的实现）。
//!
//! 指标体系：
//!   - http_requests_total{method, path, status}  请求计数
//!   - http_request_duration_seconds{method, path} 请求延迟直方图
//!   - eval_runs_total{status}                     run 完成计数
//!   - eval_queue_depth                            当前排队 run 数
//!   - eval_active_runs                            当前运行中 run 数
//!   - eval_run_duration_seconds{status}           run 总耗时直方图
//!   - eval_run_cost_usd{status}                   run 成本直方图
//!   - trace_store_append_total                    轨迹写入次数
//!
//! 暴露端点：GET /metrics（Prometheus text 格式）

use metrics::{counter, gauge, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::sync::OnceLock;

static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// 幂等初始化：多次调用返回同一个 handle（集成测试安全）
pub fn install() -> PrometheusHandle {
    HANDLE
        .get_or_init(|| {
            PrometheusBuilder::new()
                .install_recorder()
                .expect("install prometheus recorder")
        })
        .clone()
}

// ── HTTP 指标 ─────────────────────────────────────────────────────────────

pub fn http_request(method: &str, path: &str, status: u16, duration_secs: f64) {
    counter!("http_requests_total",
        "method" => method.to_string(),
        "path" => path.to_string(),
        "status" => status.to_string()
    )
    .increment(1);

    histogram!("http_request_duration_seconds",
        "method" => method.to_string(),
        "path" => path.to_string()
    )
    .record(duration_secs);
}

// ── Run 生命周期指标 ──────────────────────────────────────────────────────

pub fn run_completed(status: &str, duration_secs: f64, cost_usd: Option<f32>) {
    counter!("eval_runs_total",
        "status" => status.to_string()
    )
    .increment(1);

    histogram!("eval_run_duration_seconds",
        "status" => status.to_string()
    )
    .record(duration_secs);

    if let Some(cost) = cost_usd {
        histogram!("eval_run_cost_usd",
            "status" => status.to_string()
        )
        .record(cost as f64);
    }
}

/// 每次 reaper 循环后更新队列/活跃深度快照
pub fn update_queue_gauges(queued: i64, active: i64) {
    gauge!("eval_queue_depth").set(queued as f64);
    gauge!("eval_active_runs").set(active as f64);
}

/// 轨迹存储写入计数（监控写放大）
pub fn trace_append(lines: usize) {
    counter!("trace_store_append_total").increment(1);
    histogram!("trace_store_lines_per_batch").record(lines as f64);
}

/// 事件摄取计数（监控流量）
pub fn events_ingested(accepted: usize, rejected: usize) {
    counter!("events_ingested_total",
        "outcome" => "accepted"
    )
    .increment(accepted as u64);
    counter!("events_ingested_total",
        "outcome" => "rejected"
    )
    .increment(rejected as u64);
}
