use anyhow::Result;
use ex04_llm_api::AnthropicClient;
use ex06_tool_use::{default_registry, ToolContext};
use ex07_agent_loop::AgentLoop;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        eprintln!("ANTHROPIC_API_KEY not set — cannot run live agent loop demo.");
        return Ok(());
    }

    let llm = Arc::new(AnthropicClient::from_env()?);
    let registry = Arc::new(default_registry());

    let agent = AgentLoop {
        llm,
        registry,
        ctx: ToolContext {
            cwd: std::env::current_dir()?,
            session_id: uuid::Uuid::new_v4().to_string(),
            depth: 0,
        },
        system: "你是一个仔细的项目分析助手。先用工具查看再回答。".into(),
        model: "claude-opus-4-7".into(),
        max_tokens: 2048,
        max_iterations: 8,
        temperature: 0.0,
    };

    let run = agent
        .run("分析当前项目的结构，告诉我这是什么项目、入口在哪、主要模块是什么。")
        .await?;

    println!(
        "\n=== FINAL ({} iterations, {} in / {} out tokens) ===\n{}",
        run.iterations,
        run.total_usage.input_tokens,
        run.total_usage.output_tokens,
        run.final_text
    );
    Ok(())
}
