//! agent-eval-platform API server binary（书第 50 章）
//! 模块定义在 lib.rs；此处只做启动配置。

use eval_server::{api, metrics, scheduler, store, stream, AppState};

use std::sync::Arc;
use std::time::Duration;

use axum::http::HeaderValue;
use axum::middleware;
use axum::routing::get;
use axum::Router;
use sqlx::postgres::PgPoolOptions;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=warn".into()),
        )
        .json()
        .init();

    // ── 配置 ──────────────────────────────────────────────────────────
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:dev@localhost:5432/evalplatform".into());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let runner_secret = std::env::var("RUNNER_SECRET").ok();
    if runner_secret.is_none() {
        tracing::warn!("RUNNER_SECRET not set — /internal routes are unauthenticated (dev mode)");
    }

    // ── 数据库 ────────────────────────────────────────────────────────
    let db = PgPoolOptions::new()
        .max_connections(16)
        .min_connections(2)
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(300))
        .connect(&database_url)
        .await?;
    sqlx::migrate!("./migrations").run(&db).await?;
    tracing::info!("database migrations applied");

    // ── 对象存储 ──────────────────────────────────────────────────────
    let traces = store::TraceStore::from_env()?;

    // ── Prometheus ────────────────────────────────────────────────────
    let prom = metrics::install();

    // ── 应用状态 ──────────────────────────────────────────────────────
    let state = AppState {
        db: db.clone(),
        traces,
        hub: Arc::new(stream::StreamHub::default()),
        lease_ttl: Duration::from_secs(120),
        runner_secret,
        prom,
    };

    // ── 后台任务 ──────────────────────────────────────────────────────
    tokio::spawn(scheduler::reaper_loop(db.clone()));

    // ── HTTP 路由 ─────────────────────────────────────────────────────
    let app = Router::new()
        .nest("/api", api::public_routes())
        .nest(
            "/internal",
            api::internal_routes().route_layer(middleware::from_fn_with_state(
                state.clone(),
                api::runner_auth,
            )),
        )
        .route("/metrics", get(api::metrics_handler))
        .route("/healthz/live", get(|| async { "ok" }))
        .route("/healthz/ready", get(api::ready))
        .layer(TraceLayer::new_for_http())
        .layer(cors_layer()?)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "eval-server listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

fn cors_layer() -> anyhow::Result<CorsLayer> {
    let raw = std::env::var("CORS_ALLOWED_ORIGINS")
        .unwrap_or_else(|_| "http://localhost:5173,http://127.0.0.1:5173".to_string());
    let origins = raw
        .split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .map(HeaderValue::from_str)
        .collect::<Result<Vec<_>, _>>()?;

    if origins.is_empty() {
        anyhow::bail!("CORS_ALLOWED_ORIGINS must contain at least one origin");
    }

    Ok(CorsLayer::new()
        .allow_origin(origins)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any))
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };
    #[cfg(unix)]
    let term = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
    tracing::info!("shutdown signal received, draining in-flight requests...");
}
