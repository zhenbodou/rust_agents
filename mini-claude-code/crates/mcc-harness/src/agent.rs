//! 企业级 AgentLoop。
//!
//! 核心改造：
//!   1. `run_streaming()` — 使用 LLM 流式 API，实时发送 `AgentEvent` 到 channel
//!   2. `run()` — 非流式批量模式（headless / eval 用）
//!   3. 权限检查：每次工具调用前走 PermissionChecker
//!   4. 成本追踪：根据 model 名称计算 USD，写入 TurnEnd 事件
//!   5. Budget 限制：超出 max_usd_per_session / max_iterations 时提前终止
//!   6. 指数退避重试：LLM API 出现 5xx / 429 时自动重试，最多 3 次
//!   7. Session 录制：每轮结束写一条 TurnSnapshot 到 JSONL

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use futures::StreamExt;
use mcc_core::{AgentError, AgentEvent, ContentBlock, Message, Role, ToolContext, Usage};
use mcc_llm::{CompleteRequest, LlmProvider, StreamEvent};
use mcc_session::{SessionRecorder, TurnSnapshot};
use mcc_tools::ToolRegistry;
use tokio::sync::mpsc;
use tracing::{debug, info, instrument, warn};

use crate::permission::{Action, Decision, PermissionChecker, PermissionRequest};

// ──────────────────────────────────────────────────────────────────────────────
// Public types
// ──────────────────────────────────────────────────────────────────────────────

pub struct AgentLoop {
    pub llm: Arc<dyn LlmProvider>,
    pub registry: Arc<ToolRegistry>,
    pub ctx: ToolContext,
    pub system: String,
    pub model: String,
    pub max_tokens: u32,
    pub max_iterations: u32,
    pub temperature: f32,
    pub permission: Option<Arc<PermissionChecker>>,
    pub recorder: Option<Arc<SessionRecorder>>,
    /// USD spend cap; 0.0 = unlimited
    pub max_cost_usd: f64,
}

pub struct AgentRun {
    pub final_text: String,
    pub iterations: u32,
    pub total_usage: Usage,
    pub cost_usd: f64,
}

// ──────────────────────────────────────────────────────────────────────────────
// Token pricing (USD per 1M tokens, input / output)
// ──────────────────────────────────────────────────────────────────────────────

fn price_usd(model: &str, usage: &Usage) -> f64 {
    let (in_m, out_m) = model_price_per_1m(model);
    (usage.input_tokens as f64 / 1_000_000.0) * in_m
        + (usage.output_tokens as f64 / 1_000_000.0) * out_m
}

// ──────────────────────────────────────────────────────────────────────────────
// Permission mapping
// ──────────────────────────────────────────────────────────────────────────────

/// Map a tool call (name + its real input arguments) to the correct
/// `PermissionRequest`.
///
/// This is the bridge between the *tool* layer and the *permission* layer.
/// It MUST pass the real command / path to the checker — otherwise deny rules
/// like `Bash(rm:*)` or `Write(/etc/**)` can never match and the permission
/// system is silently bypassed.
fn permission_request_for(tool_name: &str, input: &serde_json::Value) -> PermissionRequest {
    let path_arg = || {
        input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".")
            .to_string()
    };
    match tool_name {
        // Read-only tools → Read category, gated on the target path.
        "read_file" | "list_dir" | "grep" | "glob" => PermissionRequest {
            category: "Read".into(),
            action: Action::Path { path: path_arg() },
        },
        "write_file" => PermissionRequest {
            category: "Write".into(),
            action: Action::Path { path: path_arg() },
        },
        "edit_file" => PermissionRequest {
            category: "Edit".into(),
            action: Action::Path { path: path_arg() },
        },
        "run_bash" => PermissionRequest {
            category: "Bash".into(),
            action: Action::Bash {
                cmd: input
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            },
        },
        // Unknown tool → treat as a privileged action so it defaults to "Ask"
        // (fail-safe: an unrecognized capability is never auto-allowed).
        other => PermissionRequest {
            category: "Bash".into(),
            action: Action::Bash {
                cmd: other.to_string(),
            },
        },
    }
}

fn model_price_per_1m(model: &str) -> (f64, f64) {
    if model.contains("claude-opus-4") || model.contains("opus-4") {
        (15.0, 75.0)
    } else if model.contains("claude-fable-5") || model.contains("fable-5") {
        (3.0, 15.0)
    } else if model.contains("claude-sonnet-4") || model.contains("sonnet-4") {
        (3.0, 15.0)
    } else if model.contains("claude-haiku-4") || model.contains("haiku-4") {
        (0.80, 4.0)
    } else if model.contains("gpt-4o") {
        (2.5, 10.0)
    } else if model.contains("gpt-4-turbo") {
        (10.0, 30.0)
    } else if model.contains("gpt-3.5") {
        (0.5, 1.5)
    } else {
        (3.0, 15.0) // conservative estimate
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// AgentLoop impl
// ──────────────────────────────────────────────────────────────────────────────

impl AgentLoop {
    // ── Streaming run ──────────────────────────────────────────────────────────

    /// Run the agent using the streaming API.
    /// Emits `AgentEvent`s in real-time; returns final statistics.
    // user_input 是 `impl Into<String>`，没有 Debug bound，必须加入 skip 列表，
    // 否则 #[instrument] 展开时会生成 debug!(user_input) 导致 E0277 编译错误。
    #[instrument(skip(self, tx, user_input), fields(session = %self.ctx.session_id, model = %self.model))]
    pub async fn run_streaming(
        &self,
        user_input: impl Into<String>,
        tx: &mpsc::UnboundedSender<AgentEvent>,
    ) -> Result<AgentRun, AgentError> {
        let user_input = user_input.into();
        let _ = tx.send(AgentEvent::UserEcho(user_input.clone()));

        let mut messages: Vec<Message> = vec![Message::user(&user_input)];
        let mut total_usage = Usage::default();
        let mut total_cost = 0.0f64;

        for iter in 1..=self.max_iterations {
            // Budget guard (checked at start of each iteration)
            if self.max_cost_usd > 0.0 && total_cost >= self.max_cost_usd {
                let _ = tx.send(AgentEvent::Notice(format!(
                    "Budget limit ${:.4} reached after {} iteration(s)",
                    self.max_cost_usd,
                    iter - 1
                )));
                return Err(AgentError::Budget(format!(
                    "cost ${total_cost:.4} ≥ limit ${:.4}",
                    self.max_cost_usd
                )));
            }

            debug!(iter, "starting LLM call");

            let req = CompleteRequest {
                model: self.model.clone(),
                max_tokens: self.max_tokens,
                messages: messages.clone(),
                system: Some(self.system.clone()),
                temperature: Some(self.temperature),
                tools: Some(self.registry.as_api_schema()),
            };

            let (blocks, stop_reason, turn_usage) = self.call_with_retry(&req, tx).await?;

            let turn_cost = price_usd(&self.model, &turn_usage);
            total_cost += turn_cost;
            total_usage.input_tokens += turn_usage.input_tokens;
            total_usage.output_tokens += turn_usage.output_tokens;
            total_usage.cache_read_input_tokens += turn_usage.cache_read_input_tokens;
            total_usage.cache_creation_input_tokens += turn_usage.cache_creation_input_tokens;

            // Add assistant turn to history
            let prior_messages = messages.clone();
            messages.push(Message {
                role: Role::Assistant,
                content: blocks.clone(),
            });

            match stop_reason.as_deref().unwrap_or("end_turn") {
                "end_turn" | "stop_sequence" => {
                    let final_text = blocks
                        .iter()
                        .filter_map(|b| {
                            if let ContentBlock::Text { text, .. } = b {
                                Some(text.as_str())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n");

                    let _ = tx.send(AgentEvent::TurnEnd {
                        cost_usd: total_cost,
                    });
                    info!(
                        iter,
                        total_cost,
                        input_tokens = total_usage.input_tokens,
                        output_tokens = total_usage.output_tokens,
                        "agent finished"
                    );

                    self.maybe_record_turn(
                        &prior_messages,
                        &blocks,
                        &[],
                        &turn_usage,
                        iter,
                        turn_cost,
                    )
                    .await;

                    return Ok(AgentRun {
                        final_text,
                        iterations: iter,
                        total_usage,
                        cost_usd: total_cost,
                    });
                }
                "tool_use" => {
                    let (tool_result_blocks, tool_records) =
                        self.execute_tools_streaming(&blocks, tx).await;

                    self.maybe_record_turn(
                        &prior_messages,
                        &blocks,
                        &tool_records,
                        &turn_usage,
                        iter,
                        turn_cost,
                    )
                    .await;

                    messages.push(Message {
                        role: Role::User,
                        content: tool_result_blocks,
                    });
                }
                other => {
                    warn!(stop = other, "unexpected stop reason");
                    return Err(AgentError::Api(format!("unexpected stop_reason: {other}")));
                }
            }
        }

        Err(AgentError::Budget(format!(
            "exceeded max_iterations={}",
            self.max_iterations
        )))
    }

    // ── Headless batch run ─────────────────────────────────────────────────────

    /// Non-streaming run. Discards events internally. Suitable for batch / eval.
    pub async fn run(&self, user_input: impl Into<String>) -> Result<AgentRun, AgentError> {
        let (tx, mut rx) = mpsc::unbounded_channel::<AgentEvent>();
        let result = self.run_streaming(user_input, &tx).await;
        drop(tx);
        while rx.try_recv().is_ok() {}
        result
    }

    // ── Retry wrapper ─────────────────────────────────────────────────────────

    async fn call_with_retry(
        &self,
        req: &CompleteRequest,
        tx: &mpsc::UnboundedSender<AgentEvent>,
    ) -> Result<(Vec<ContentBlock>, Option<String>, Usage), AgentError> {
        const MAX_RETRIES: u32 = 3;
        let mut attempt = 0u32;
        loop {
            match self.call_streaming_once(req, tx).await {
                Ok(r) => return Ok(r),
                Err(e) => {
                    attempt += 1;
                    let msg = e.to_string();
                    let retryable = msg.contains("429")
                        || msg.contains("500")
                        || msg.contains("502")
                        || msg.contains("503")
                        || msg.contains("overloaded");

                    if !retryable || attempt >= MAX_RETRIES {
                        return Err(AgentError::Api(msg));
                    }

                    let delay = Duration::from_millis(500 * (1u64 << attempt));
                    warn!(attempt, ?delay, "LLM error, retrying: {msg}");
                    let _ = tx.send(AgentEvent::Notice(format!(
                        "API error (attempt {attempt}/{MAX_RETRIES}), retrying in {delay:?}…"
                    )));
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    /// Single streaming LLM call → (blocks, stop_reason, usage).
    ///
    /// Tool call blocks are assembled in **stream order** (insertion order) so
    /// the assistant message presented to the API is deterministic and matches
    /// the order the model intended.  A plain `HashMap` would give arbitrary
    /// ordering across runs, which can confuse APIs that validate ordering.
    async fn call_streaming_once(
        &self,
        req: &CompleteRequest,
        tx: &mpsc::UnboundedSender<AgentEvent>,
    ) -> Result<(Vec<ContentBlock>, Option<String>, Usage)> {
        let mut stream = self.llm.stream(req.clone()).await?;

        let mut text_buf = String::new();
        // Ordered list: (tool_use_id, name, accumulated_json_buffer).
        // Vec preserves stream insertion order; index tracked separately.
        let mut tool_slots: Vec<(String, String, String)> = Vec::new();
        let mut current_tool_idx: Option<usize> = None;
        let mut stop_reason = "end_turn".to_string();
        let mut usage = Usage::default();

        while let Some(ev) = stream.next().await {
            let ev = ev.map_err(|e| anyhow::anyhow!("{e}"))?;
            match ev {
                StreamEvent::TextDelta(t) => {
                    let _ = tx.send(AgentEvent::TextDelta(t.clone()));
                    text_buf.push_str(&t);
                }
                StreamEvent::ToolUseStart { id, name } => {
                    current_tool_idx = Some(tool_slots.len());
                    let _ = tx.send(AgentEvent::ToolCallStart {
                        id: id.clone(),
                        name: name.clone(),
                        args_preview: String::new(),
                    });
                    tool_slots.push((id, name, String::new()));
                }
                StreamEvent::ToolUseInputDelta(partial) => {
                    if let Some(idx) = current_tool_idx {
                        tool_slots[idx].2.push_str(&partial);
                    }
                }
                StreamEvent::MessageStop {
                    stop_reason: sr,
                    usage: u,
                } => {
                    stop_reason = sr;
                    usage = u;
                }
            }
        }

        // Assemble ContentBlocks in stream order
        let mut blocks: Vec<ContentBlock> = Vec::new();
        if !text_buf.is_empty() {
            blocks.push(ContentBlock::Text {
                text: text_buf,
                cache_control: None,
            });
        }
        for (id, name, json_str) in &tool_slots {
            let input: serde_json::Value = serde_json::from_str(json_str)
                .unwrap_or(serde_json::Value::Object(Default::default()));
            blocks.push(ContentBlock::ToolUse {
                id: id.clone(),
                name: name.clone(),
                input,
            });
        }

        Ok((blocks, Some(stop_reason), usage))
    }

    // ── Parallel tool execution ────────────────────────────────────────────────

    async fn execute_tools_streaming(
        &self,
        blocks: &[ContentBlock],
        tx: &mpsc::UnboundedSender<AgentEvent>,
    ) -> (Vec<ContentBlock>, Vec<(String, String, bool)>) {
        let calls: Vec<(String, String, serde_json::Value)> = blocks
            .iter()
            .filter_map(|b| {
                if let ContentBlock::ToolUse { id, name, input } = b {
                    Some((id.clone(), name.clone(), input.clone()))
                } else {
                    None
                }
            })
            .collect();

        // Pair each JoinHandle with its tool_use_id so we can produce a
        // proper error ToolResult even if the spawned task panics.
        let mut id_handles: Vec<(String, tokio::task::JoinHandle<(String, String, bool)>)> =
            Vec::new();

        for (id, name, input) in calls {
            let reg = self.registry.clone();
            let ctx = self.ctx.clone();
            let perm = self.permission.clone();
            let tool_name = name.clone();
            let tool_id = id.clone();

            let handle = tokio::spawn(async move {
                // Permission check — map the tool + its REAL arguments to the
                // correct category/action so deny/allow rules actually apply.
                let decision = if let Some(p) = &perm {
                    p.check(&permission_request_for(&tool_name, &input))
                } else {
                    Decision::Allow
                };

                let (content, is_error) = match decision {
                    Decision::Deny(reason) => (format!("permission denied: {reason}"), true),
                    Decision::Ask(msg) => (
                        format!("interactive permission required (headless): {msg}"),
                        true,
                    ),
                    Decision::Allow => match reg.get(&tool_name) {
                        Some(t) => {
                            let t0 = Instant::now();
                            let out = t.execute(input, &ctx).await;
                            debug!(
                                tool = %tool_name,
                                elapsed_ms = t0.elapsed().as_millis(),
                                is_error = out.is_error,
                                "tool done"
                            );
                            (out.content, out.is_error)
                        }
                        None => (format!("unknown tool: {tool_name}"), true),
                    },
                };
                (tool_id, content, is_error)
            });
            id_handles.push((id, handle));
        }

        let mut result_blocks = Vec::new();
        let mut records: Vec<(String, String, bool)> = Vec::new();

        for (fallback_id, h) in id_handles {
            let (id, content, is_error) = match h.await {
                Ok(result) => result,
                Err(join_err) => {
                    // The tool task panicked. Use the pre-captured id so we
                    // still send a valid ToolResult back to the LLM rather
                    // than silently dropping it (which would leave the
                    // assistant message with an unmatched tool_use block).
                    warn!(id = %fallback_id, "tool task panicked: {join_err}");
                    (
                        fallback_id,
                        "internal error: tool task panicked".into(),
                        true,
                    )
                }
            };
            let _ = tx.send(AgentEvent::ToolCallEnd {
                id: id.clone(),
                output: content.chars().take(500).collect(),
                is_error,
            });
            records.push((id.clone(), content.clone(), is_error));
            result_blocks.push(ContentBlock::ToolResult {
                tool_use_id: id,
                content,
                is_error,
            });
        }

        (result_blocks, records)
    }

    // ── Session recording ──────────────────────────────────────────────────────

    async fn maybe_record_turn(
        &self,
        request_messages: &[Message],
        assistant_blocks: &[ContentBlock],
        tool_outputs: &[(String, String, bool)],
        usage: &Usage,
        iteration: u32,
        cost_usd: f64,
    ) {
        if let Some(rec) = &self.recorder {
            let snap = TurnSnapshot {
                ts: chrono::Utc::now(),
                iteration,
                request_messages: request_messages.to_vec(),
                assistant_blocks: assistant_blocks.to_vec(),
                tool_outputs: tool_outputs.to_vec(),
                usage: *usage,
                model: self.model.clone(),
                cost_usd,
            };
            if let Err(e) = rec.record(snap).await {
                warn!("session record error: {e}");
            }
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Builder
// ──────────────────────────────────────────────────────────────────────────────

pub struct AgentLoopBuilder {
    llm: Arc<dyn LlmProvider>,
    registry: Arc<ToolRegistry>,
    ctx: ToolContext,
    system: String,
    model: String,
    max_tokens: u32,
    max_iterations: u32,
    temperature: f32,
    permission: Option<Arc<PermissionChecker>>,
    recorder: Option<Arc<SessionRecorder>>,
    max_cost_usd: f64,
}

impl AgentLoopBuilder {
    pub fn new(llm: Arc<dyn LlmProvider>, registry: Arc<ToolRegistry>, ctx: ToolContext) -> Self {
        Self {
            llm,
            registry,
            ctx,
            system: String::new(),
            model: "claude-sonnet-4-6".into(),
            max_tokens: 8192,
            max_iterations: 40,
            temperature: 0.0,
            permission: None,
            recorder: None,
            max_cost_usd: 0.0,
        }
    }

    pub fn system(mut self, s: impl Into<String>) -> Self {
        self.system = s.into();
        self
    }
    pub fn model(mut self, m: impl Into<String>) -> Self {
        self.model = m.into();
        self
    }
    pub fn max_tokens(mut self, n: u32) -> Self {
        self.max_tokens = n;
        self
    }
    pub fn max_iterations(mut self, n: u32) -> Self {
        self.max_iterations = n;
        self
    }
    pub fn temperature(mut self, t: f32) -> Self {
        self.temperature = t;
        self
    }
    pub fn permission(mut self, p: Arc<PermissionChecker>) -> Self {
        self.permission = Some(p);
        self
    }
    pub fn recorder(mut self, r: Arc<SessionRecorder>) -> Self {
        self.recorder = Some(r);
        self
    }
    pub fn max_cost_usd(mut self, usd: f64) -> Self {
        self.max_cost_usd = usd;
        self
    }

    pub fn build(self) -> AgentLoop {
        AgentLoop {
            llm: self.llm,
            registry: self.registry,
            ctx: self.ctx,
            system: self.system,
            model: self.model,
            max_tokens: self.max_tokens,
            max_iterations: self.max_iterations,
            temperature: self.temperature,
            permission: self.permission,
            recorder: self.recorder,
            max_cost_usd: self.max_cost_usd,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use futures::stream::{self, BoxStream};
    use mcc_core::{ContentBlock, Usage};
    use mcc_llm::{CompleteRequest, LlmProvider, MessageResponse, StreamEvent};
    use mcc_tools::default_registry;
    use tempfile::TempDir;

    struct MockLlm {
        text: String,
    }

    #[async_trait]
    impl LlmProvider for MockLlm {
        async fn complete(&self, _req: CompleteRequest) -> anyhow::Result<MessageResponse> {
            Ok(MessageResponse {
                content: vec![ContentBlock::Text {
                    text: self.text.clone(),
                    cache_control: None,
                }],
                stop_reason: Some("end_turn".into()),
                usage: Usage {
                    input_tokens: 10,
                    output_tokens: 5,
                    ..Default::default()
                },
            })
        }

        async fn stream(
            &self,
            _req: CompleteRequest,
        ) -> anyhow::Result<BoxStream<'static, anyhow::Result<StreamEvent>>> {
            let text = self.text.clone();
            let events: Vec<anyhow::Result<StreamEvent>> = vec![
                Ok(StreamEvent::TextDelta(text)),
                Ok(StreamEvent::MessageStop {
                    stop_reason: "end_turn".into(),
                    usage: Usage {
                        input_tokens: 10,
                        output_tokens: 5,
                        ..Default::default()
                    },
                }),
            ];
            Ok(Box::pin(stream::iter(events)))
        }
    }

    fn make_ctx(dir: &TempDir) -> ToolContext {
        ToolContext {
            cwd: dir.path().to_path_buf(),
            session_id: "test-session".into(),
            depth: 0,
        }
    }

    #[tokio::test]
    async fn test_streaming_end_turn_events() {
        let dir = TempDir::new().unwrap();
        let llm = Arc::new(MockLlm {
            text: "Hello, world!".into(),
        });
        let registry = Arc::new(default_registry());

        let agent = AgentLoopBuilder::new(llm, registry, make_ctx(&dir))
            .system("You are a test agent.")
            .model("claude-haiku-4-5-20251001")
            .build();

        let (tx, mut rx) = mpsc::unbounded_channel();
        let run = agent.run_streaming("hi", &tx).await.unwrap();

        assert_eq!(run.final_text, "Hello, world!");
        assert_eq!(run.iterations, 1);

        let mut events = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            events.push(ev);
        }

        assert!(events.iter().any(|e| matches!(e, AgentEvent::UserEcho(_))));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::TextDelta(_))));
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::TurnEnd { .. })));
    }

    #[tokio::test]
    async fn test_headless_run() {
        let dir = TempDir::new().unwrap();
        let llm = Arc::new(MockLlm { text: "42".into() });
        let registry = Arc::new(default_registry());

        let agent = AgentLoopBuilder::new(llm, registry, make_ctx(&dir)).build();
        let run = agent.run("what is 6×7?").await.unwrap();
        assert_eq!(run.final_text, "42");
    }

    #[tokio::test]
    async fn test_cost_calculation_nonzero() {
        let dir = TempDir::new().unwrap();
        // 10in + 5out tokens with haiku pricing: 0.80/4.0 per 1M
        let llm = Arc::new(MockLlm {
            text: "done".into(),
        });
        let registry = Arc::new(default_registry());

        let agent = AgentLoopBuilder::new(llm, registry, make_ctx(&dir))
            .model("claude-haiku-4-5-20251001")
            .build();
        let run = agent.run("test").await.unwrap();
        // 10 input × 0.80/1M + 5 output × 4.0/1M = 0.000008 + 0.00002 = 0.000028 USD
        assert!(run.cost_usd > 0.0);
    }

    // ── Permission mapping (regression tests for the gating bug) ───────────────

    use crate::permission::PermissionChecker;
    use mcc_config::PermissionConfig;
    use serde_json::json;

    #[test]
    fn perm_request_maps_tools_to_correct_category() {
        // read-only tools → Read + the real path
        let r = permission_request_for("read_file", &json!({"path": "src/main.rs"}));
        assert_eq!(r.category, "Read");
        assert!(matches!(&r.action, Action::Path { path } if path == "src/main.rs"));

        // write/edit → Write/Edit
        let w = permission_request_for("write_file", &json!({"path": "out.txt"}));
        assert_eq!(w.category, "Write");
        let e = permission_request_for("edit_file", &json!({"path": "out.txt"}));
        assert_eq!(e.category, "Edit");

        // bash → the REAL command, not the tool name
        let b = permission_request_for("run_bash", &json!({"command": "rm -rf /"}));
        assert_eq!(b.category, "Bash");
        assert!(matches!(&b.action, Action::Bash { cmd } if cmd == "rm -rf /"));
    }

    #[test]
    fn deny_rule_blocks_dangerous_command_through_mapping() {
        // Before the fix, the checker saw `cmd = "run_bash"`, so `Bash(rm:*)`
        // never matched and `rm -rf /` slipped through. This locks the fix.
        let cfg = PermissionConfig {
            mode: None,
            allow: vec![],
            deny: vec!["Bash(rm:*)".into()],
        };
        let checker = PermissionChecker::new(&cfg).unwrap();

        let req = permission_request_for("run_bash", &json!({"command": "rm -rf /"}));
        assert!(matches!(checker.check(&req), Decision::Deny(_)));
    }

    #[test]
    fn read_is_auto_allowed_so_headless_can_inspect_files() {
        // Default mode: reads must be allowed, otherwise headless mode would
        // fail on every read_file call.
        let cfg = PermissionConfig {
            mode: None,
            allow: vec![],
            deny: vec![],
        };
        let checker = PermissionChecker::new(&cfg).unwrap();

        let req = permission_request_for("read_file", &json!({"path": "a.txt"}));
        assert!(matches!(checker.check(&req), Decision::Allow));
    }
}
