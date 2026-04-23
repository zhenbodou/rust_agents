# 第 21 章 CLI / TUI 与消息渲染

> 先给 Agent 一副**能交互的脸**。我们用 `clap` + `ratatui` + `crossterm` 做一个 Claude Code 风格的终端界面。

## 21.1 CLI 入口：`crates/mcc-cli`

### 21.1.1 子命令设计

```bash
mcc                       # 启动 TUI REPL
mcc -p "fix bug in x.rs"  # 单次非交互模式（给 CI / script 用）
mcc resume <session_id>   # 恢复 session
mcc sessions list         # 列出所有 session
mcc eval run              # 跑 eval 集
mcc config show           # 显示合并后的配置
```

### 21.1.2 Clap 结构

`crates/mcc-cli/src/cli.rs`：

```rust
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "mcc", version, about = "Mini Claude Code (Rust)")]
pub struct Cli {
    /// 单次模式：直接把 prompt 发给 Agent 跑完就退出
    #[arg(short, long)]
    pub prompt: Option<String>,

    /// 项目根目录（默认当前 cwd）
    #[arg(long, env = "MCC_PROJECT")]
    pub cwd: Option<PathBuf>,

    /// 覆盖权限模式
    #[arg(long)]
    pub permission_mode: Option<String>,

    #[command(subcommand)]
    pub cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// 恢复已有 session
    Resume { session_id: String },

    /// Session 管理
    Sessions {
        #[command(subcommand)]
        op: SessionOp,
    },

    /// 显示合并后的配置
    Config {
        #[command(subcommand)]
        op: ConfigOp,
    },

    /// 运行 evals
    Eval {
        #[command(subcommand)]
        op: EvalOp,
    },
}

#[derive(Subcommand, Debug)]
pub enum SessionOp { List, Show { id: String } }

#[derive(Subcommand, Debug)]
pub enum ConfigOp { Show }

#[derive(Subcommand, Debug)]
pub enum EvalOp { Run { #[arg(long)] suite: PathBuf } }
```

### 21.1.3 main.rs

```rust
use anyhow::Result;
use clap::Parser;
use cli::{Cli, Cmd};

mod cli;
mod tui_app;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let args = Cli::parse();
    mcc_harness::observability::init("mcc");

    let cwd = args.cwd.unwrap_or_else(|| std::env::current_dir().unwrap());
    let config = mcc_config::load(&cwd).await?;

    match args.cmd {
        None => {
            if let Some(p) = args.prompt {
                run_headless(config, cwd, p).await
            } else {
                tui_app::run(config, cwd).await
            }
        }
        Some(Cmd::Resume { session_id }) => tui_app::resume(config, cwd, session_id).await,
        Some(Cmd::Sessions { op }) => handle_sessions(op, &cwd).await,
        Some(Cmd::Config { op: cli::ConfigOp::Show }) => {
            println!("{}", serde_json::to_string_pretty(&config)?);
            Ok(())
        }
        Some(Cmd::Eval { op: cli::EvalOp::Run { suite } }) => {
            mcc_harness::evals::run_suite_cli(&suite).await
        }
    }
}

async fn run_headless(config: mcc_config::Config, cwd: PathBuf, prompt: String) -> Result<()> {
    let agent = mcc_harness::build_agent(&config, cwd).await?;
    let run = agent.run(prompt).await?;
    println!("{}", run.final_text);
    eprintln!("\n[usage: {}in / {}out, ${:.4}]",
        run.total_usage.input_tokens, run.total_usage.output_tokens,
        mcc_harness::cost::estimate_usd(&run.total_usage));
    Ok(())
}
```

## 21.2 TUI：Ratatui 骨架

我们要的效果：

```text
╭─ mini-claude-code ─ session 1a2b (cost $0.034) ──╮
│ > 你好，读一下 src/main.rs                         │
│                                                   │
│ [read_file src/main.rs]   ✓ 42 lines              │
│                                                   │
│ ◎ 这是一个 clap + tokio 的 CLI 入口……              │
│                                                   │
│                                                   │
╰───────────────────────────────────────────────────╯
 [Enter: send · Esc: cancel · /help · Ctrl-C: quit]
```

### 21.2.1 App state

```rust
pub struct App {
    pub session: SessionHandle,
    pub input: String,
    pub cursor: u16,
    pub items: Vec<RenderItem>,
    pub streaming: Option<String>,   // 正在接收的 assistant text
    pub agent_busy: bool,
    pub cost_usd: f64,
    pub cancel_token: tokio_util::sync::CancellationToken,
}

pub enum RenderItem {
    User(String),
    AssistantText(String),
    ToolCall { name: String, args: String, status: CallStatus },
    ToolResult { name: String, output: String, is_error: bool },
    Notice(String),
}

pub enum CallStatus { Pending, Running, Done, Failed }
```

### 21.2.2 事件循环

```rust
pub async fn run_tui(mut app: App) -> anyhow::Result<()> {
    let mut terminal = ratatui::init();
    let mut events = crossterm::event::EventStream::new();
    let mut agent_events = app.session.subscribe_agent_events(); // tokio broadcast

    loop {
        terminal.draw(|f| ui(f, &app))?;

        tokio::select! {
            Some(Ok(ev)) = events.next() => {
                if handle_key(&mut app, ev).await? { break; }
            }
            Ok(e) = agent_events.recv() => {
                apply_agent_event(&mut app, e);
            }
        }
    }
    ratatui::restore();
    Ok(())
}
```

### 21.2.3 UI 渲染（简化）

```rust
fn ui(f: &mut ratatui::Frame, app: &App) {
    use ratatui::{layout::*, widgets::*, style::*, text::*};
    let size = f.area();
    let chunks = Layout::vertical([
        Constraint::Min(3),       // 消息区
        Constraint::Length(3),    // 输入框
        Constraint::Length(1),    // 状态栏
    ]).split(size);

    // --- 消息区 ---
    let lines: Vec<Line> = app.items.iter().flat_map(render_item).collect();
    let msg = Paragraph::new(lines).block(
        Block::bordered().title(format!(
            " mini-claude-code — session {} (${:.3}) ",
            &app.session.short_id(), app.cost_usd))
    ).wrap(Wrap { trim: false });
    f.render_widget(msg, chunks[0]);

    // --- 输入框 ---
    let input = Paragraph::new(app.input.as_str())
        .block(Block::bordered().title(" input "));
    f.render_widget(input, chunks[1]);
    f.set_cursor_position((chunks[1].x + 1 + app.cursor, chunks[1].y + 1));

    // --- 状态栏 ---
    let tips = if app.agent_busy {
        Line::from(vec![Span::styled(" ●", Style::new().fg(Color::Yellow)), Span::raw(" thinking… (Esc to cancel)")])
    } else {
        Line::from(" Enter: send · /help · Ctrl-C: quit ")
    };
    f.render_widget(Paragraph::new(tips), chunks[2]);
}

fn render_item(item: &RenderItem) -> Vec<Line<'_>> {
    match item {
        RenderItem::User(t) => vec![Line::from(vec![Span::styled("> ", Style::new().fg(Color::Cyan)), Span::raw(t)])],
        RenderItem::AssistantText(t) => vec![Line::from(vec![Span::styled("◎ ", Style::new().fg(Color::Green)), Span::raw(t)])],
        RenderItem::ToolCall { name, args, status } => {
            let icon = match status { CallStatus::Pending=>"…", CallStatus::Running=>"▶", CallStatus::Done=>"✓", CallStatus::Failed=>"✗" };
            vec![Line::from(format!("  [{icon}] {name}({})", truncate(args, 80)))]
        }
        RenderItem::ToolResult { name: _, output, is_error } => {
            let style = if *is_error { Style::new().fg(Color::Red) } else { Style::new().fg(Color::DarkGray) };
            output.lines().take(10).map(|l| Line::styled(format!("    {l}"), style)).collect()
        }
        RenderItem::Notice(t) => vec![Line::styled(format!("· {t}"), Style::new().fg(Color::Blue))],
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n { s.to_string() } else { format!("{}…", &s[..n]) }
}
```

### 21.2.4 键位

```rust
async fn handle_key(app: &mut App, ev: crossterm::event::Event) -> anyhow::Result<bool> {
    use crossterm::event::{Event, KeyCode, KeyModifiers};
    if let Event::Key(k) = ev {
        match (k.code, k.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => return Ok(true),
            (KeyCode::Esc, _) if app.agent_busy => { app.cancel_token.cancel(); }
            (KeyCode::Enter, _) if !app.agent_busy => {
                let input = std::mem::take(&mut app.input);
                app.cursor = 0;
                if !input.trim().is_empty() {
                    app.items.push(RenderItem::User(input.clone()));
                    app.agent_busy = true;
                    app.session.submit(input);
                }
            }
            (KeyCode::Char(c), _) => { app.input.insert(app.cursor as usize, c); app.cursor += 1; }
            (KeyCode::Backspace, _) if app.cursor > 0 => {
                app.cursor -= 1; app.input.remove(app.cursor as usize);
            }
            _ => {}
        }
    }
    Ok(false)
}
```

## 21.3 Agent 事件的流式投递

`SessionHandle::subscribe_agent_events()` 返回 `tokio::sync::broadcast::Receiver<AgentEvent>`。Agent 主循环（下一章）把每个增量事件 send 到通道：

```rust
pub enum AgentEvent {
    UserEcho(String),
    TextDelta(String),
    ToolCallStart { id: String, name: String, args_preview: String },
    ToolCallEnd { id: String, output: String, is_error: bool },
    TurnEnd { cost_usd: f64 },
    Notice(String),
    Error(String),
}
```

这样 TUI 能实时拼接字符流并更新显示。

## 21.4 单次模式与 TUI 共享代码

两种模式只有"渲染器"不同：

- TUI 用上面的广播订阅
- Headless 用一个简单的 stdout writer（见 `run_headless`）

这就是 `AgentEvent` 抽象的价值——**前端可插拔**。

## 21.5 小结

- `clap` 提供 CLI 子命令；`ratatui` + `crossterm` 画 TUI
- 核心抽象：`AgentEvent` 事件流，前端只消费事件，不直接调 Agent
- Esc 可打断、Ctrl-C 退出、Enter 提交——UX 的细节

> **下一章**：为 Agent 实现完整的工具集。

