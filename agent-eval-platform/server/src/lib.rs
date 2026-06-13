//! eval-server 库入口（供集成测试 import）。
//! binary 入口在 main.rs。

pub mod api;
pub mod domain;
pub mod error;
pub mod metrics;
pub mod scheduler;
pub mod store;
pub mod stream;

use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub traces: store::TraceStore,
    pub hub: Arc<stream::StreamHub>,
    pub lease_ttl: Duration,
    pub runner_secret: Option<String>,
    pub prom: metrics_exporter_prometheus::PrometheusHandle,
}
