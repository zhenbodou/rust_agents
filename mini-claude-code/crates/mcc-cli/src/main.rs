//! mcc — Mini Claude Code CLI 入口（企业级版本）
//!
//! 模式：
//!   mcc                         → 启动交互 TUI（默认）
//!   mcc -p "prompt"             → headless 单次模式，打印结果到 stdout
//!   mcc tui                     → 强制 TUI 模式
//!   mcc config                  → 打印合并后的配置（JSON）
//!   mcc version                 → 打印版本号
//!
//! 环境变量：
//!   ANTHROPIC_API_KEY           → 使用 Anthropic Claude
//!   OPENAI_API_KEY              → 使用 OpenAI（兼容任意 OpenAI-compatible 端点）
//!   OPENAI_BASE_URL             → 可选，指向兼容网关（DeepSeek、Ollama、…）
//!   MCC_PROJECT                 → 覆盖工作目录
//!   MODEL                       → 覆盖模型名称
//!   RUST_LOG                    → 日志等级（默认 info）

use anyhow::Result;
use clap::{Parser, Subcommand};
use mcc_core::{AgentEvent, ToolContext};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::info;

#[derive(Parser, Debug)]
#[command(
    name = "mcc",
    version,
    about = "Mini Claude Code — enterprise-grade AI coding assistant (Rust)",
)]
struct Cli {
    /// 单次 headless 模式：执行 prompt 后退出（非零退出码 = 错误）
    #[arg(short, long)]
    prompt: Option<String>,

    /// 工作目录（默认：当前目录）
    #[arg(long, env = "MCC_PROJECT")]
    cwd: Option<PathBuf>,

    /// 覆盖模型（e.g. claude-opus-4-7, gpt-4o）
    #[arg(long, env = "MODEL")]
    model: Option<String>,

    /// 静默模式：只输出最终答案，不输出统计信息
    #[arg(long)]
    quiet: bool,

    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// 启动交互 TUI
    Tui,
    /// 打印合并后的配置（JSON）
    Config,
    /// 打印版本号
    Version,
}

// ──────────────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    // Structured logging to stderr (stdout stays clean for headless output)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .with_target(false)
        .compact()
        .init();

    let args = Cli::parse();
    let cwd = args
        .cwd
        .clone()
        .unwrap_or_else(|| std::env::current_dir().expect("cannot read cwd"));

    let config = mcc_config::load(&cwd).await?;

    match args.cmd {
        Some(Cmd::Version) => {
            println!("mcc {}", env!("CARGO_PKG_VERSION"));
        }
        Some(Cmd::Config) => {
            println!("{}", serde_json::to_string_pretty(&config)?);
        }
        Some(Cmd::Tui) => {
            run_tui_mode(config, cwd, args.model).await?;
        }
        None => {
            if let Some(prompt) = args.prompt {
                run_headless(config, cwd, prompt, args.model, args.quiet).await?;
            } else {
                run_tui_mode(config, cwd, args.model).await?;
            }
        }
    }

    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// Headless mode
// ──────────────────────────────────────────────────────────────────────────────

async fn run_headless(
    config: mcc_config::Config,
    cwd: PathBuf,
    prompt: String,
    model_override: Option<String>,
    quiet: bool,
) -> Result<()> {
    let llm = mcc_llm::auto_provider_from_env().map_err(|e| {
        eprintln!("Error: {e}");
        eprintln!("Set ANTHROPIC_API_KEY or OPENAI_API_KEY.");
        e
    })?;

    let registry = Arc::new(mcc_tools::default_registry());
    let session_id = uuid::Uuid::new_v4().to_string();
    let model = model_override.unwrap_or(config.model.main.clone());
    let permission = Arc::new(build_permission(&config)?);
    let ctx = ToolContext { cwd, session_id: session_id.clone(), depth: 0 };

    let agent = mcc_harness::AgentLoopBuilder::new(llm, registry, ctx)
        .system(system_prompt())
        .model(model)
        .max_tokens(8192)
        .max_iterations(config.budget.max_iterations)
        .max_cost_usd(config.budget.max_usd_per_session)
        .permission(permission)
        .build();

    let (tx, mut rx) = mpsc::unbounded_channel::<AgentEvent>();

    // Print tool events to stderr while agent runs
    let quiet2 = quiet;
    let printer = tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            match ev {
                AgentEvent::ToolCallStart { name, .. } => {
                    if !quiet2 {
                        eprintln!("  ⚙  {name}…");
                    }
                }
                AgentEvent::ToolCallEnd { is_error: true, output, .. } => {
                    if !quiet2 {
                        eprintln!("  ✗  tool error: {}", output.chars().take(120).collect::<String>());
                    }
                }
                AgentEvent::Notice(msg) => {
                    if !quiet2 {
                        eprintln!("  ℹ  {msg}");
                    }
                }
                AgentEvent::Error(e) => {
                    eprintln!("  ✗  {e}");
                }
                _ => {}
            }
        }
    });

    let result = agent.run_streaming(prompt, &tx).await;
    drop(tx);
    printer.await.ok();

    match result {
        Ok(run) => {
            println!("{}", run.final_text);
            if !quiet {
                eprintln!(
                    "\n[{iter} iter | {in_tok}↑ {out_tok}↓ tokens | ${cost:.5} USD]",
                    iter    = run.iterations,
                    in_tok  = run.total_usage.input_tokens,
                    out_tok = run.total_usage.output_tokens,
                    cost    = run.cost_usd,
                );
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("Agent error: {e}");
            std::process::exit(1);
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// TUI mode
// ──────────────────────────────────────────────────────────────────────────────

async fn run_tui_mode(
    config: mcc_config::Config,
    cwd: PathBuf,
    model_override: Option<String>,
) -> Result<()> {
    let llm = match mcc_llm::auto_provider_from_env() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Error: {e}");
            eprintln!("Set ANTHROPIC_API_KEY or OPENAI_API_KEY.");
            std::process::exit(1);
        }
    };

    let (event_tx, event_rx) = mpsc::unbounded_channel::<AgentEvent>();
    let (input_tx, mut input_rx) = mpsc::unbounded_channel::<String>();

    let session_id = uuid::Uuid::new_v4().to_string();
    let short_id = session_id[..8].to_string();
    let model = model_override.unwrap_or(config.model.main.clone());

    info!(session_id, model, "starting TUI session");

    let permission = Arc::new(build_permission(&config)?);
    let max_iter = config.budget.max_iterations;
    let max_cost = config.budget.max_usd_per_session;
    let cwd_task = cwd.clone();
    let model_task = model.clone();

    // Background agent task: handles one message per loop iteration
    tokio::spawn(async move {
        let registry = Arc::new(mcc_tools::default_registry());

        while let Some(user_input) = input_rx.recv().await {
            let agent = mcc_harness::AgentLoopBuilder::new(
                llm.clone(),
                registry.clone(),
                ToolContext {
                    cwd: cwd_task.clone(),
                    session_id: session_id.clone(),
                    depth: 0,
                },
            )
            .system(system_prompt())
            .model(model_task.clone())
            .max_tokens(8192)
            .max_iterations(max_iter)
            .max_cost_usd(max_cost)
            .permission(permission.clone())
            .build();

            match agent.run_streaming(user_input, &event_tx).await {
                Ok(run) => {
                    info!(cost = run.cost_usd, iter = run.iterations, "turn complete");
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

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

fn system_prompt() -> String {
    "You are mini-claude-code, an enterprise-grade AI coding assistant. \
     Examine files with read_file or list_dir before making changes. \
     Use edit_file for targeted modifications. \
     Use run_bash for shell tasks. \
     Use grep to search code. \
     Be concise and precise."
        .to_string()
}

fn build_permission(config: &mcc_config::Config) -> Result<mcc_harness::PermissionChecker> {
    mcc_harness::PermissionChecker::new(&config.permissions)
        .map_err(|e| anyhow::anyhow!("permission config error: {e}"))
}
