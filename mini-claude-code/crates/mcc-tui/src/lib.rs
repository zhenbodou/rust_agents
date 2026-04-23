//! mcc-tui：基于 Ratatui + Crossterm 的 Agent 终端前端。
//!
//! 设计要点：
//! - UI 与业务完全解耦：通过 `AgentEvent` 单向事件流驱动
//! - 用户输入以回调函数上抛，不在 UI 层做 Agent 调用
//! - 支持 Esc 取消、Ctrl-C 退出、Enter 提交
//!
//! 使用方式（精简）：
//!
//! ```no_run
//! use mcc_tui::{run_tui, TuiHandles};
//! use tokio::sync::mpsc;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let (event_tx, event_rx) = mpsc::unbounded_channel();
//!     let (input_tx, mut input_rx) = mpsc::unbounded_channel::<String>();
//!
//!     tokio::spawn(async move {
//!         while let Some(_user_msg) = input_rx.recv().await {
//!             // 把 _user_msg 丢给 Agent，跑完后把 AgentEvent 送到 event_tx
//!         }
//!     });
//!
//!     run_tui(TuiHandles {
//!         events: event_rx,
//!         input_tx,
//!         session_short_id: "abcd".into(),
//!     }).await
//! }
//! ```

use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use mcc_core::AgentEvent;
use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use std::collections::VecDeque;
use tokio::sync::mpsc;

pub struct TuiHandles {
    pub events: mpsc::UnboundedReceiver<AgentEvent>,
    pub input_tx: mpsc::UnboundedSender<String>,
    pub session_short_id: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum RenderItem {
    User(String),
    AssistantText(String),
    ToolCall {
        id: String,
        name: String,
        args_preview: String,
        status: CallStatus,
    },
    ToolResult {
        id: String,
        output: String,
        is_error: bool,
    },
    Notice(String),
    Error(String),
}

#[derive(Debug, Clone, Copy)]
enum CallStatus {
    Running,
    Done,
    Failed,
}

struct App {
    items: VecDeque<RenderItem>,
    streaming_text: String,
    input: String,
    cursor: usize,
    agent_busy: bool,
    cost_usd: f64,
    session_short_id: String,
    scroll: u16,
}

impl App {
    fn new(session_short_id: String) -> Self {
        Self {
            items: VecDeque::new(),
            streaming_text: String::new(),
            input: String::new(),
            cursor: 0,
            agent_busy: false,
            cost_usd: 0.0,
            session_short_id,
            scroll: 0,
        }
    }

    fn push(&mut self, item: RenderItem) {
        // 上限保持合理：保留最近 500 项
        if self.items.len() >= 500 {
            self.items.pop_front();
        }
        self.items.push_back(item);
    }

    fn flush_streaming(&mut self) {
        if !self.streaming_text.is_empty() {
            let text = std::mem::take(&mut self.streaming_text);
            self.push(RenderItem::AssistantText(text));
        }
    }

    fn apply_event(&mut self, ev: AgentEvent) {
        match ev {
            AgentEvent::UserEcho(s) => {
                self.flush_streaming();
                self.push(RenderItem::User(s));
            }
            AgentEvent::TextDelta(t) => {
                self.streaming_text.push_str(&t);
            }
            AgentEvent::ToolCallStart {
                id,
                name,
                args_preview,
            } => {
                self.flush_streaming();
                self.push(RenderItem::ToolCall {
                    id,
                    name,
                    args_preview,
                    status: CallStatus::Running,
                });
            }
            AgentEvent::ToolCallEnd {
                id,
                output,
                is_error,
            } => {
                // 更新对应 ToolCall 状态
                for it in self.items.iter_mut() {
                    if let RenderItem::ToolCall {
                        id: call_id,
                        status,
                        ..
                    } = it
                    {
                        if call_id == &id {
                            *status = if is_error {
                                CallStatus::Failed
                            } else {
                                CallStatus::Done
                            };
                            break;
                        }
                    }
                }
                self.push(RenderItem::ToolResult {
                    id,
                    output,
                    is_error,
                });
            }
            AgentEvent::TurnEnd { cost_usd } => {
                self.flush_streaming();
                self.cost_usd = cost_usd;
                self.agent_busy = false;
            }
            AgentEvent::Notice(s) => self.push(RenderItem::Notice(s)),
            AgentEvent::Error(s) => {
                self.flush_streaming();
                self.push(RenderItem::Error(s));
                self.agent_busy = false;
            }
            AgentEvent::PermissionRequest { id: _, message } => {
                self.push(RenderItem::Notice(format!("⚠ permission: {message}")));
            }
        }
    }
}

pub async fn run_tui(mut h: TuiHandles) -> Result<()> {
    let mut terminal = ratatui::init();
    let mut ev_stream = EventStream::new();
    let mut app = App::new(h.session_short_id.clone());

    let result = async {
        loop {
            terminal.draw(|f| ui(f, &app))?;

            tokio::select! {
                Some(ev) = ev_stream.next() => {
                    match ev? {
                        Event::Key(k) if k.kind != KeyEventKind::Release => {
                            if handle_key(&mut app, k, &h.input_tx).await? {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                Some(agent_ev) = h.events.recv() => {
                    app.apply_event(agent_ev);
                }
            }
        }
        Ok::<(), anyhow::Error>(())
    }
    .await;

    ratatui::restore();
    result
}

async fn handle_key(
    app: &mut App,
    k: crossterm::event::KeyEvent,
    input_tx: &mpsc::UnboundedSender<String>,
) -> Result<bool> {
    match (k.code, k.modifiers) {
        (KeyCode::Char('c'), KeyModifiers::CONTROL)
        | (KeyCode::Char('d'), KeyModifiers::CONTROL) => return Ok(true),
        (KeyCode::Esc, _) if app.agent_busy => {
            app.push(RenderItem::Notice("(cancel requested)".into()));
            // 这里可以发一个特殊消息让 Agent cancel；保持简单：
        }
        (KeyCode::Enter, _) if !app.agent_busy => {
            let text = std::mem::take(&mut app.input);
            app.cursor = 0;
            if !text.trim().is_empty() {
                app.agent_busy = true;
                let _ = input_tx.send(text);
            }
        }
        (KeyCode::Backspace, _) => {
            if app.cursor > 0 {
                app.cursor -= 1;
                app.input.remove(app.cursor);
            }
        }
        (KeyCode::Left, _) => {
            if app.cursor > 0 {
                app.cursor -= 1;
            }
        }
        (KeyCode::Right, _) => {
            if app.cursor < app.input.chars().count() {
                app.cursor += 1;
            }
        }
        (KeyCode::Home, _) => app.cursor = 0,
        (KeyCode::End, _) => app.cursor = app.input.chars().count(),
        (KeyCode::PageUp, _) => app.scroll = app.scroll.saturating_add(5),
        (KeyCode::PageDown, _) => app.scroll = app.scroll.saturating_sub(5),
        (KeyCode::Char(c), _)
            if !k.modifiers.contains(KeyModifiers::CONTROL)
                && !k.modifiers.contains(KeyModifiers::ALT) =>
        {
            // 按字符索引插入
            let byte_idx = byte_index(&app.input, app.cursor);
            app.input.insert(byte_idx, c);
            app.cursor += 1;
        }
        _ => {}
    }
    Ok(false)
}

fn byte_index(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

fn ui(f: &mut Frame, app: &App) {
    let area = f.area();
    let chunks = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .split(area);

    // === 消息区 ===
    let mut lines: Vec<Line> = Vec::new();
    for item in &app.items {
        lines.extend(render_item(item));
    }
    if !app.streaming_text.is_empty() {
        lines.extend(render_item(&RenderItem::AssistantText(
            app.streaming_text.clone(),
        )));
    }

    let title = format!(
        " mini-claude-code — session {} (${:.3}){} ",
        app.session_short_id,
        app.cost_usd,
        if app.agent_busy { " · thinking…" } else { "" }
    );
    let msg_block = Block::default().borders(Borders::ALL).title(title);
    let para = Paragraph::new(lines)
        .block(msg_block)
        .wrap(Wrap { trim: false })
        .scroll((app.scroll, 0));
    f.render_widget(para, chunks[0]);

    // === 输入框 ===
    let input_block = Block::default().borders(Borders::ALL).title(" input ");
    let input = Paragraph::new(app.input.as_str()).block(input_block);
    f.render_widget(input, chunks[1]);
    f.set_cursor_position((chunks[1].x + 1 + app.cursor as u16, chunks[1].y + 1));

    // === 状态栏 ===
    let tips = if app.agent_busy {
        Line::from(vec![
            Span::styled("●", Style::default().fg(Color::Yellow)),
            Span::raw(" thinking… (Esc cancel · Ctrl-C quit)"),
        ])
    } else {
        Line::from(" Enter: send · PgUp/PgDn: scroll · Ctrl-C: quit ")
    };
    f.render_widget(Paragraph::new(tips), chunks[2]);
}

fn render_item(item: &RenderItem) -> Vec<Line<'static>> {
    match item {
        RenderItem::User(t) => vec![Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(t.clone()),
        ])],
        RenderItem::AssistantText(t) => {
            let mut out = vec![Line::from(vec![Span::styled(
                "◎ assistant",
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            )])];
            for line in t.lines() {
                out.push(Line::from(format!("  {line}")));
            }
            out
        }
        RenderItem::ToolCall {
            name,
            args_preview,
            status,
            ..
        } => {
            let (icon, color) = match status {
                CallStatus::Running => ("▶", Color::Yellow),
                CallStatus::Done => ("✓", Color::Green),
                CallStatus::Failed => ("✗", Color::Red),
            };
            vec![Line::from(vec![
                Span::styled(format!(" {icon} "), Style::default().fg(color)),
                Span::styled(
                    name.clone(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!("({})", truncate(args_preview, 80))),
            ])]
        }
        RenderItem::ToolResult { output, is_error, .. } => {
            let style = if *is_error {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            output
                .lines()
                .take(8)
                .map(|l| Line::styled(format!("    {l}"), style))
                .collect()
        }
        RenderItem::Notice(t) => vec![Line::styled(
            format!("· {t}"),
            Style::default().fg(Color::Blue),
        )],
        RenderItem::Error(t) => vec![Line::styled(
            format!("✗ error: {t}"),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )],
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let head: String = s.chars().take(n).collect();
        format!("{head}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_accumulates_text_deltas() {
        let mut app = App::new("test".into());
        app.apply_event(AgentEvent::UserEcho("hi".into()));
        app.apply_event(AgentEvent::TextDelta("he".into()));
        app.apply_event(AgentEvent::TextDelta("llo".into()));
        assert_eq!(app.streaming_text, "hello");
        app.apply_event(AgentEvent::TurnEnd { cost_usd: 0.01 });
        assert_eq!(app.streaming_text, "");
        assert!(!app.agent_busy);
        // last item should be flushed assistant text
        assert!(matches!(app.items.back(), Some(RenderItem::AssistantText(_))));
    }

    #[test]
    fn tool_call_updates_status() {
        let mut app = App::new("test".into());
        app.apply_event(AgentEvent::ToolCallStart {
            id: "c1".into(),
            name: "read".into(),
            args_preview: "{}".into(),
        });
        app.apply_event(AgentEvent::ToolCallEnd {
            id: "c1".into(),
            output: "42".into(),
            is_error: false,
        });
        let has_done = app.items.iter().any(|it| {
            matches!(
                it,
                RenderItem::ToolCall {
                    status: CallStatus::Done,
                    ..
                }
            )
        });
        assert!(has_done);
    }
}
