# 第 23 章 Agent 主循环与流式输出

> 把第 7 章的 AgentLoop 升级到**生产级**：流式事件、取消、预算、重试、观测。

## 23.1 事件驱动的循环

与第 7 章最大的不同：**边 stream 边派发事件**给 TUI，用户能看到打字效果和工具进度。

```rust
pub struct ProductionAgent {
    pub llm: Arc<dyn LlmProvider>,
    pub registry: Arc<ToolRegistry>,
    pub ctx: ToolContext,
    pub system: String,
    pub model: String,
    pub config: AgentConfig,
    pub recorder: Arc<SessionRecorder>,
    pub dispatcher: Arc<HookDispatcher>,
    pub event_tx: tokio::sync::mpsc::UnboundedSender<AgentEvent>,
    pub cancel: tokio_util::sync::CancellationToken,
    pub cost: Arc<Mutex<CostTracker>>,
}

pub struct AgentConfig {
    pub max_iterations: u32,
    pub max_tokens_per_call: u32,
    pub budget_usd: f64,
    pub temperature: f32,
    pub retry: RetryPolicy,
}
```

## 23.2 单轮执行

```rust
impl ProductionAgent {
    pub async fn run(&self, user_input: String) -> Result<AgentRun, AgentError> {
        let mut messages = self.load_and_prepend_context(user_input.clone()).await?;
        let _ = self.event_tx.send(AgentEvent::UserEcho(user_input));

        for iter in 1..=self.config.max_iterations {
            self.check_budget()?;
            self.check_cancelled()?;

            let req = CompleteRequest {
                model: self.model.clone(),
                max_tokens: self.config.max_tokens_per_call,
                messages: messages.clone(),
                system: Some(self.system.clone()),
                temperature: Some(self.config.temperature),
                tools: Some(self.registry.as_api_schema()),
            };

            let (assistant_blocks, stop_reason, usage) = self.stream_one_turn(req).await?;

            {
                let mut c = self.cost.lock().unwrap();
                c.add(&self.model, usage);
            }
            messages.push(Message { role: Role::Assistant, content: assistant_blocks.clone() });

            match stop_reason.as_str() {
                "end_turn" | "stop_sequence" => {
                    let final_text = extract_text(&assistant_blocks);
                    let total = { self.cost.lock().unwrap().clone() };
                    let _ = self.event_tx.send(AgentEvent::TurnEnd { cost_usd: total.estimated_usd() });
                    self.recorder.record(TurnSnapshot { /* ... */ }).await?;
                    return Ok(AgentRun { final_text, messages, iterations: iter, total_usage: total.into() });
                }
                "tool_use" => {
                    let results = self.execute_tool_calls(&assistant_blocks).await;
                    messages.push(Message { role: Role::User, content: results });
                }
                "max_tokens" => {
                    return Err(AgentError::Other(anyhow::anyhow!("assistant hit max_tokens")));
                }
                other => return Err(AgentError::Other(anyhow::anyhow!("unexpected stop: {other}"))),
            }
        }
        Err(AgentError::Other(anyhow::anyhow!("exceeded max_iterations")))
    }

    fn check_budget(&self) -> Result<(), AgentError> {
        let c = self.cost.lock().unwrap();
        if c.estimated_usd() > self.config.budget_usd {
            return Err(AgentError::Budget(format!("${:.3} > ${:.3}", c.estimated_usd(), self.config.budget_usd)));
        }
        Ok(())
    }
    fn check_cancelled(&self) -> Result<(), AgentError> {
        if self.cancel.is_cancelled() {
            return Err(AgentError::Other(anyhow::anyhow!("cancelled by user")));
        }
        Ok(())
    }
}
```

## 23.3 流式聚合

`stream_one_turn` 把 SSE 事件**同时**做两件事：

1. 拼回完整 `ContentBlock[]`（给 messages 用）
2. 逐块推送 `AgentEvent` 给 UI

```rust
async fn stream_one_turn(&self, req: CompleteRequest) -> Result<(Vec<ContentBlock>, String, Usage), AgentError> {
    let mut stream = with_retry(&self.config.retry, || self.llm.stream(req.clone())).await?;

    let mut blocks: Vec<ContentBlock> = Vec::new();
    let mut current_text = String::new();
    let mut current_tool: Option<(String, String, String)> = None;  // (id, name, json_buf)
    let mut stop_reason = String::new();
    let mut usage = Usage::default();

    use futures::StreamExt;
    while let Some(ev) = stream.next().await {
        if self.cancel.is_cancelled() { return Err(AgentError::Other(anyhow::anyhow!("cancelled"))); }
        match ev? {
            StreamEvent::TextDelta(t) => {
                let _ = self.event_tx.send(AgentEvent::TextDelta(t.clone()));
                current_text.push_str(&t);
            }
            StreamEvent::ToolUseStart { id, name } => {
                // flush text
                if !current_text.is_empty() {
                    blocks.push(ContentBlock::Text { text: std::mem::take(&mut current_text), cache_control: None });
                }
                let _ = self.event_tx.send(AgentEvent::ToolCallStart { id: id.clone(), name: name.clone(), args_preview: String::new() });
                current_tool = Some((id, name, String::new()));
            }
            StreamEvent::ToolUseInputDelta(partial) => {
                if let Some((_, _, ref mut buf)) = current_tool {
                    buf.push_str(&partial);
                }
            }
            StreamEvent::MessageStop { stop_reason: sr, usage: u } => {
                stop_reason = sr; usage = u;
            }
        }
    }

    if !current_text.is_empty() { blocks.push(ContentBlock::Text { text: current_text, cache_control: None }); }
    if let Some((id, name, buf)) = current_tool {
        let input: serde_json::Value = serde_json::from_str(&buf).unwrap_or(serde_json::json!({}));
        blocks.push(ContentBlock::ToolUse { id, name, input });
    }

    Ok((blocks, stop_reason, usage))
}
```

## 23.4 工具并行执行 + Hooks

```rust
async fn execute_tool_calls(&self, blocks: &[ContentBlock]) -> Vec<ContentBlock> {
    let calls: Vec<_> = blocks.iter().filter_map(|b| {
        if let ContentBlock::ToolUse { id, name, input } = b {
            Some((id.clone(), name.clone(), input.clone()))
        } else { None }
    }).collect();

    let mut tasks = Vec::with_capacity(calls.len());
    for (id, name, input) in calls {
        let reg = self.registry.clone();
        let ctx = self.ctx.clone();
        let dispatcher = self.dispatcher.clone();
        let tx = self.event_tx.clone();
        let session = ctx.session_id.clone();

        tasks.push(tokio::spawn(async move {
            // PreToolUse
            let pre = dispatcher.dispatch(HookEvent::PreToolUse {
                session_id: session.clone(),
                tool_name: name.clone(),
                input: input.clone(),
            }).await;
            if pre.block {
                let msg = pre.reason.unwrap_or_else(|| "blocked by hook".into());
                let _ = tx.send(AgentEvent::ToolCallEnd { id: id.clone(), output: msg.clone(), is_error: true });
                return ContentBlock::ToolResult { tool_use_id: id, content: msg, is_error: true };
            }

            let (content, is_error) = match reg.get(&name) {
                Some(t) => {
                    let out = t.execute(input.clone(), &ctx).await;
                    (out.content, out.is_error)
                }
                None => (format!("unknown tool: {name}"), true),
            };

            // PostToolUse
            let post = dispatcher.dispatch(HookEvent::PostToolUse {
                session_id: session,
                tool_name: name.clone(),
                output: content.clone(),
                is_error,
            }).await;
            let final_content = post.replace_output.unwrap_or(content);

            let _ = tx.send(AgentEvent::ToolCallEnd { id: id.clone(), output: final_content.clone(), is_error });
            ContentBlock::ToolResult { tool_use_id: id, content: final_content, is_error }
        }));
    }

    let mut results = Vec::new();
    for t in tasks { if let Ok(r) = t.await { results.push(r); } }
    results
}
```

## 23.5 上下文装配

```rust
async fn load_and_prepend_context(&self, user_input: String) -> Result<Vec<Message>, AgentError> {
    // 1. HookEvent::UserPromptSubmit，可能会 inject
    let resp = self.dispatcher.dispatch(HookEvent::UserPromptSubmit {
        session_id: self.ctx.session_id.clone(),
        prompt: user_input.clone(),
    }).await;
    if resp.block {
        return Err(AgentError::Other(anyhow::anyhow!("hook blocked: {}", resp.reason.unwrap_or_default())));
    }

    let mut messages = Vec::new();
    if let Some(extra) = resp.inject {
        messages.push(Message::user(extra));
    }
    messages.push(Message::user(user_input));
    Ok(messages)
}
```

## 23.6 取消与清理

用户按 Esc → `cancel_token.cancel()`。主循环下一次 `check_cancelled()` 或 stream 检测到后退出。**正在跑的 bash 子进程**必须能被 kill——`run_bash` 用 `.kill_on_drop(true)` 自动处理（Tokio 在 drop 时发 SIGKILL）。

更优雅：先 SIGTERM 等 2s 再 SIGKILL。生产可用 [`nix`](https://crates.io/crates/nix)：

```rust
if let Some(pid) = child.id() {
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), nix::sys::signal::SIGTERM).ok();
    tokio::time::sleep(Duration::from_secs(2)).await;
    let _ = child.start_kill();
}
```

## 23.7 端到端冒烟

`tests/smoke.rs`：

```rust
#[tokio::test]
async fn agent_reads_and_summarizes() -> anyhow::Result<()> {
    if std::env::var("ANTHROPIC_API_KEY").is_err() { eprintln!("skip (no key)"); return Ok(()); }

    let tmp = tempfile::tempdir()?;
    tokio::fs::write(tmp.path().join("hello.txt"), "hello rust agents").await?;

    let agent = build_test_agent(tmp.path()).await?;
    let run = agent.run("读一下 hello.txt 然后告诉我这是什么。".into()).await?;
    assert!(run.final_text.to_lowercase().contains("rust"));
    Ok(())
}
```

## 23.8 小结

- 流式 = 一边拼 blocks 一边 send events
- 并行 tool call + Pre/Post Hook 夹击
- 取消 / 预算 / 重试 贯穿整个循环
- 到此你的 Agent 已经能真正写代码了

> **下一章**：把权限 + Hooks 真正接入（示例配置 + 脚本）。

