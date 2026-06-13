# 第 21 章 CLI / TUI 与消息渲染

> 先给 Agent 一副**能交互的脸**。我们用 `clap` + `ratatui` + `crossterm` 做一个 Claude Code 风格的终端界面。

## 21.1 CLI 入口：`crates/mcc-cli`

### 21.1.1 子命令设计

```bash
mcc                              # 启动 TUI REPL（默认）
mcc -p "list the files here"     # headless 单次模式（CI / 脚本）
mcc -p "..." --quiet             # 仅输出最终答案，不打印统计
mcc --model gpt-4o               # 覆盖模型
mcc tui                          # 显式启动 TUI
mcc config                       # 打印合并后的配置（JSON）
mcc version                      # 打印版本号
```

### 21.1.2 Clap 结构

`crates/mcc-cli/src/main.rs`：

```rust
#[derive(Parser, Debug)]
#[command(name = "mcc", version,
          about = "Mini Claude Code — enterprise-grade AI coding assistant (Rust)")]
struct Cli {
    /// 单次 headless 模式（非零退出码 = 错误）
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
    Tui,      // 显式 TUI 模式
    Config,   // 打印合并配置
    Version,  // 打印版本
}
```

### 21.1.3 main.rs 完整实现

```rust
#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    // 日志输出到 stderr（保持 stdout 干净，headless 模式可 pipe 输出）
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(std::io::stderr)
        .with_target(false)
        .compact()
        .init();

    let args = Cli::parse();
    let cwd = args.cwd.clone()
        .unwrap_or_else(|| std::env::current_dir().expect("cannot read cwd"));
    let config = mcc_config::load(&cwd).await?;

    match args.cmd {
        Some(Cmd::Version) => println!("mcc {}", env!("CARGO_PKG_VERSION")),
        Some(Cmd::Config) => println!("{}", serde_json::to_string_pretty(&config)?),
        Some(Cmd::Tui)    => run_tui_mode(config, cwd, args.model).await?,
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
```

### 21.1.4 headless 模式

headless 模式的关键：**stdout 输出最终答案，stderr 输出进度/统计**。这样 `mcc -p "..." | other-command` 可以无缝管道。

```rust
async fn run_headless(
    config: mcc_config::Config, cwd: PathBuf,
    prompt: String, model_override: Option<String>, quiet: bool,
) -> Result<()> {
    let llm = mcc_llm::auto_provider_from_env()?;
    let registry = Arc::new(mcc_tools::default_registry());
    let permission = Arc::new(PermissionChecker::new(&config.permissions)?);

    let agent = AgentLoopBuilder::new(llm, registry,
        ToolContext { cwd, session_id: Uuid::new_v4().to_string(), depth: 0 })
        .system(system_prompt())
        .model(model_override.unwrap_or(config.model.main.clone()))
        .max_tokens(8192)
        .max_iterations(config.budget.max_iterations)
        .max_cost_usd(config.budget.max_usd_per_session)
        .permission(permission)
        .build();

    let (tx, mut rx) = mpsc::unbounded_channel::<AgentEvent>();

    // 实时打印工具进度到 stderr
    let printer = tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            match ev {
                AgentEvent::ToolCallStart { name, .. } if !quiet =>
                    eprintln!("  ⚙  {name}…"),
                AgentEvent::ToolCallEnd { is_error: true, output, .. } =>
                    eprintln!("  ✗  {}", output.chars().take(120).collect::<String>()),
                AgentEvent::Notice(msg) if !quiet =>
                    eprintln!("  ℹ  {msg}"),
                AgentEvent::Error(e) => eprintln!("  ✗  {e}"),
                _ => {}
            }
        }
    });

    let result = agent.run_streaming(prompt, &tx).await;
    drop(tx);
    printer.await.ok();

    match result {
        Ok(run) => {
            println!("{}", run.final_text);   // ← stdout（可 pipe）
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
        Err(e) => { eprintln!("Agent error: {e}"); std::process::exit(1); }
    }
}
```

**效果示例：**

```bash
$ mcc -p "count lines in src/main.rs" --quiet
288

$ mcc -p "list all .rs files" 2>/dev/null | wc -l
12

$ ANTHROPIC_API_KEY=sk-ant-... mcc -p "write a hello world to /tmp/hw.rs"
  ⚙  write_file…
  ⚙  run_bash…

[1 iter | 1234↑ 89↓ tokens | $0.00422 USD]
Done! Created /tmp/hw.rs.
```

### 21.1.5 TUI 模式

TUI 模式把 `AgentLoop` 放到后台 task，主线程跑 ratatui 事件循环：

```rust
async fn run_tui_mode(
    config: mcc_config::Config, cwd: PathBuf, model_override: Option<String>,
) -> Result<()> {
    let llm = mcc_llm::auto_provider_from_env()?;
    let (event_tx, event_rx) = mpsc::unbounded_channel::<AgentEvent>();
    let (input_tx, mut input_rx) = mpsc::unbounded_channel::<String>();

    let session_id = Uuid::new_v4().to_string();
    let model = model_override.unwrap_or(config.model.main.clone());
    let permission = Arc::new(PermissionChecker::new(&config.permissions)?);

    // 后台处理每条用户消息
    tokio::spawn(async move {
        let registry = Arc::new(mcc_tools::default_registry());
        while let Some(user_input) = input_rx.recv().await {
            let agent = AgentLoopBuilder::new(
                llm.clone(), registry.clone(),
                ToolContext { cwd: cwd.clone(), session_id: session_id.clone(), depth: 0 },
            )
            .system(system_prompt()).model(model.clone())
            .max_iterations(config.budget.max_iterations)
            .max_cost_usd(config.budget.max_usd_per_session)
            .permission(permission.clone())
            .build();

            match agent.run_streaming(user_input, &event_tx).await {
                Ok(run) => info!(cost = run.cost_usd, iter = run.iterations, "turn done"),
                Err(e) => { let _ = event_tx.send(AgentEvent::Error(e.to_string())); }
            }
        }
    });

    mcc_tui::run_tui(mcc_tui::TuiHandles {
        events: event_rx, input_tx,
        session_short_id: session_id[..8].to_string(),
    }).await
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

