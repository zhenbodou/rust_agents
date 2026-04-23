//! mini-claude-code CLI 入口。
//!
//! 使用方式：
//! - `mcc`                    启动 TUI REPL
//! - `mcc -p "..."`           单次 headless 模式
//! - `mcc config`             打印合并后的配置
//! - `mcc version`            打印版本
//!
//! LLM provider 由环境变量自动选择：
//! - 有 `ANTHROPIC_API_KEY`          → Anthropic
//! - 否则有 `OPENAI_API_KEY`         → OpenAI 兼容
//!   （DeepSeek / Kimi / Qwen / Groq / vLLM 等，通过 `OPENAI_BASE_URL` 切换）

use anyhow::Result;
use clap::{Parser, Subcommand};
use mcc_core::{AgentEvent, ToolContext};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Parser, Debug)]
#[command(name = "mcc", version, about = "Mini Claude Code (Rust)")]
struct Cli {
    /// 单次模式：直接 prompt 并退出
    #[arg(short, long)]
    prompt: Option<String>,

    /// 工作目录
    #[arg(long, env = "MCC_PROJECT")]
    cwd: Option<PathBuf>,

    /// 覆盖模型
    #[arg(long, env = "MODEL")]
    model: Option<String>,

    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// 启动交互 TUI（默认模式如无 -p）
    Tui,
    /// 打印合并后的配置
    Config,
    /// 打印版本
    Version,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let args = Cli::parse();
    let cwd = args
        .cwd
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let config = mcc_config::load(&cwd).await?;

    match args.cmd {
        Some(Cmd::Version) => {
            println!("mcc {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some(Cmd::Config) => {
            println!("{}", serde_json::to_string_pretty(&config)?);
            Ok(())
        }
        Some(Cmd::Tui) => run_tui_mode(config, cwd, args.model).await,
        None => {
            if let Some(p) = args.prompt {
                run_headless(config, cwd, p, args.model).await
            } else {
                run_tui_mode(config, cwd, args.model).await
            }
        }
    }
}

async fn run_headless(
    config: mcc_config::Config,
    cwd: PathBuf,
    prompt: String,
    model_override: Option<String>,
) -> Result<()> {
    let llm = match mcc_llm::auto_provider_from_env() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{e}");
            return Ok(());
        }
    };
    let registry = Arc::new(mcc_tools::default_registry());
    let agent = mcc_harness::AgentLoop {
        llm,
        registry,
        ctx: ToolContext {
            cwd,
            session_id: uuid::Uuid::new_v4().to_string(),
            depth: 0,
        },
        system: "你是 mini-claude-code 的助手。先用工具查看再回答。".into(),
        model: model_override.unwrap_or(config.model.main.clone()),
        max_tokens: 2048,
        max_iterations: config.budget.max_iterations,
        temperature: 0.0,
    };

    let run = agent.run(prompt).await?;
    println!("{}", run.final_text);
    eprintln!(
        "[{} iterations, {}in/{}out tokens]",
        run.iterations, run.total_usage.input_tokens, run.total_usage.output_tokens
    );
    Ok(())
}

async fn run_tui_mode(
    config: mcc_config::Config,
    cwd: PathBuf,
    model_override: Option<String>,
) -> Result<()> {
    let llm = match mcc_llm::auto_provider_from_env() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{e}");
            return Ok(());
        }
    };

    let (event_tx, event_rx) = mpsc::unbounded_channel::<AgentEvent>();
    let (input_tx, mut input_rx) = mpsc::unbounded_channel::<String>();

    let session_id = uuid::Uuid::new_v4().to_string();
    let short_id = session_id[..6].to_string();

    // 后台跑 Agent
    let cwd_task = cwd.clone();
    let model = model_override.unwrap_or(config.model.main.clone());
    let max_iterations = config.budget.max_iterations;
    tokio::spawn(async move {
        let registry = Arc::new(mcc_tools::default_registry());
        while let Some(user_input) = input_rx.recv().await {
            let _ = event_tx.send(AgentEvent::UserEcho(user_input.clone()));

            let agent = mcc_harness::AgentLoop {
                llm: llm.clone(),
                registry: registry.clone(),
                ctx: ToolContext {
                    cwd: cwd_task.clone(),
                    session_id: session_id.clone(),
                    depth: 0,
                },
                system: "你是 mini-claude-code 的助手。".into(),
                model: model.clone(),
                max_tokens: 2048,
                max_iterations,
                temperature: 0.0,
            };

            match agent.run(user_input).await {
                Ok(run) => {
                    let _ = event_tx.send(AgentEvent::TextDelta(run.final_text));
                    let _ = event_tx.send(AgentEvent::TurnEnd { cost_usd: 0.0 });
                }
                Err(e) => {
                    let _ = event_tx.send(AgentEvent::Error(e.to_string()));
                }
            }
        }
    });

    mcc_tui::run_tui(mcc_tui::TuiHandles {
        events: event_rx,
        input_tx,
        session_short_id: short_id,
    })
    .await
}
