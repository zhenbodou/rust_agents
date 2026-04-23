#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("Hello, Agent! — smoke test for the workspace");
    Ok(())
}
