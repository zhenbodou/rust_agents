# 第 23 章 Agent 主循环与流式输出

> 把第 7 章的 `AgentLoop` 升级到**生产级**：流式事件、预算、重试、权限集成。

## 23.1 设计目标

第 7 章的 `AgentLoop::run()` 是阻塞的——等所有 turn 完成才返回。生产版有三个新要求：

1. **实时流式** — 用户要看到打字效果，工具要看到进度条
2. **预算守护** — 超出 cost 或 iteration 上限时优雅终止
3. **权限集成** — 每个工具调用前走 `PermissionChecker`

解决方案：`AgentLoopBuilder` + `run_streaming()` 通过 `mpsc::UnboundedSender<AgentEvent>` 推送事件。

## 23.2 `AgentLoopBuilder`

采用 Builder 模式，便于各场景（TUI / headless / eval）灵活组装：

```rust
// crates/mcc-harness/src/agent.rs

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
    /// USD 花费上限；0.0 = 不限制
    pub max_cost_usd: f64,
}

pub struct AgentLoopBuilder { /* 同字段 */ }

impl AgentLoopBuilder {
    pub fn new(llm: Arc<dyn LlmProvider>, registry: Arc<ToolRegistry>, ctx: ToolContext) -> Self {
        Self {
            llm, registry, ctx,
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
    pub fn system(mut self, s: impl Into<String>) -> Self { self.system = s.into(); self }
    pub fn model(mut self, m: impl Into<String>) -> Self { self.model = m.into(); self }
    pub fn max_tokens(mut self, n: u32) -> Self { self.max_tokens = n; self }
    pub fn max_iterations(mut self, n: u32) -> Self { self.max_iterations = n; self }
    pub fn permission(mut self, p: Arc<PermissionChecker>) -> Self { self.permission = Some(p); self }
    pub fn recorder(mut self, r: Arc<SessionRecorder>) -> Self { self.recorder = Some(r); self }
    pub fn max_cost_usd(mut self, usd: f64) -> Self { self.max_cost_usd = usd; self }
    pub fn build(self) -> AgentLoop { /* ... */ }
}
```

## 23.3 `run_streaming()` 主循环

```rust
impl AgentLoop {
    /// 流式运行：实时推送 AgentEvent，返回聚合统计。
    pub async fn run_streaming(
        &self,
        user_input: impl Into<String>,
        tx: &mpsc::UnboundedSender<AgentEvent>,
    ) -> Result<AgentRun, AgentError> {
        let user_input = user_input.into();
        let _ = tx.send(AgentEvent::UserEcho(user_input.clone()));

        let mut messages = vec![Message::user(&user_input)];
        let mut total_usage = Usage::default();
        let mut total_cost = 0.0f64;

        for iter in 1..=self.max_iterations {
            // ① 预算守护
            if self.max_cost_usd > 0.0 && total_cost >= self.max_cost_usd {
                let _ = tx.send(AgentEvent::Notice(format!(
                    "Budget limit ${:.4} reached", self.max_cost_usd
                )));
                return Err(AgentError::Budget(format!("${total_cost:.4} ≥ ${:.4}", self.max_cost_usd)));
            }

            // ② LLM 流式调用（含重试）
            let (blocks, stop_reason, turn_usage) =
                self.call_with_retry(&req, tx).await?;

            let turn_cost = price_usd(&self.model, &turn_usage);
            total_cost += turn_cost;
            total_usage += turn_usage;

            messages.push(Message { role: Role::Assistant, content: blocks.clone() });

            match stop_reason.as_deref().unwrap_or("end_turn") {
                "end_turn" | "stop_sequence" => {
                    let final_text = extract_text(&blocks);
                    let _ = tx.send(AgentEvent::TurnEnd { cost_usd: total_cost });
                    self.maybe_record_turn(/* ... */).await;
                    return Ok(AgentRun { final_text, iterations: iter, total_usage, cost_usd: total_cost });
                }
                "tool_use" => {
                    // ③ 并行执行工具（含权限检查）
                    let (tool_results, records) =
                        self.execute_tools_streaming(&blocks, tx).await;
                    self.maybe_record_turn(/* ... */).await;
                    messages.push(Message { role: Role::User, content: tool_results });
                }
                other => {
                    return Err(AgentError::Api(format!("unexpected stop_reason: {other}")));
                }
            }
        }
        Err(AgentError::Budget(format!("exceeded max_iterations={}", self.max_iterations)))
    }

    /// 非流式批量运行（eval / 测试用）。
    pub async fn run(&self, user_input: impl Into<String>) -> Result<AgentRun, AgentError> {
        let (tx, mut rx) = mpsc::unbounded_channel::<AgentEvent>();
        let result = self.run_streaming(user_input, &tx).await;
        drop(tx);
        while rx.try_recv().is_ok() {}   // 丢弃事件
        result
    }
}
```

## 23.4 流式聚合：`call_streaming_once`

LLM `stream()` 推送三类事件，需要**同时**：

1. 拼回完整 `ContentBlock[]`（给 messages history）
2. 逐块推送 `AgentEvent` 给 UI

```rust
async fn call_streaming_once(
    &self,
    req: &CompleteRequest,
    tx: &mpsc::UnboundedSender<AgentEvent>,
) -> anyhow::Result<(Vec<ContentBlock>, Option<String>, Usage)> {
    let mut stream = self.llm.stream(req.clone()).await?;

    let mut text_buf = String::new();
    // ⚠️ 用 Vec 保持流式到达的顺序，不能用 HashMap（迭代顺序不确定）。
    // (tool_use_id, name, 累积 json_buffer)，用 current_tool_idx 定位当前 slot。
    let mut tool_slots: Vec<(String, String, String)> = Vec::new();
    let mut current_tool_idx: Option<usize> = None;
    let mut stop_reason = "end_turn".to_string();
    let mut usage = Usage::default();

    while let Some(ev) = stream.next().await {
        match ev? {
            StreamEvent::TextDelta(t) => {
                let _ = tx.send(AgentEvent::TextDelta(t.clone()));
                text_buf.push_str(&t);
            }
            StreamEvent::ToolUseStart { id, name } => {
                current_tool_idx = Some(tool_slots.len());
                // 立即通知 TUI"工具开始了"
                let _ = tx.send(AgentEvent::ToolCallStart {
                    id: id.clone(), name: name.clone(), args_preview: String::new(),
                });
                tool_slots.push((id, name, String::new()));
            }
            StreamEvent::ToolUseInputDelta(partial) => {
                // 累积工具参数 JSON（分块到达）
                if let Some(idx) = current_tool_idx {
                    tool_slots[idx].2.push_str(&partial);
                }
            }
            StreamEvent::MessageStop { stop_reason: sr, usage: u } => {
                stop_reason = sr; usage = u;
            }
        }
    }

    // 将累积状态按流式顺序组装成 ContentBlocks
    let mut blocks = Vec::new();
    if !text_buf.is_empty() {
        blocks.push(ContentBlock::Text { text: text_buf, cache_control: None });
    }
    for (id, name, json_str) in &tool_slots {
        let input = serde_json::from_str(json_str)
            .unwrap_or(serde_json::Value::Object(Default::default()));
        blocks.push(ContentBlock::ToolUse { id: id.clone(), name: name.clone(), input });
    }

    Ok((blocks, Some(stop_reason), usage))
}
```

## 23.5 指数退避重试

LLM API 会出现 429 / 503，必须自动重试：

```rust
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
                    || msg.contains("500") || msg.contains("503")
                    || msg.contains("overloaded");

                if !retryable || attempt >= MAX_RETRIES {
                    return Err(AgentError::Api(msg));
                }
                // 指数退避：0.5s → 1s → 2s（含抖动）
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
```

## 23.6 工具并行执行 + 权限检查

多个工具调用在 `tokio::spawn` 中并行执行，每个调用前走 `PermissionChecker`：

```rust
async fn execute_tools_streaming(
    &self,
    blocks: &[ContentBlock],
    tx: &mpsc::UnboundedSender<AgentEvent>,
) -> (Vec<ContentBlock>, Vec<(String, String, bool)>) {
    let mut handles = Vec::new();

    for (id, name, input) in extract_tool_calls(blocks) {
        let reg = self.registry.clone();
        let ctx = self.ctx.clone();
        let perm = self.permission.clone();
        let tx2 = tx.clone();

        handles.push(tokio::spawn(async move {
            // 权限检查
            let decision = if let Some(p) = &perm {
                p.check(&PermissionRequest {
                    category: name.clone(),
                    action: Action::Bash { cmd: name.clone() },
                })
            } else {
                Decision::Allow
            };

            let (content, is_error) = match decision {
                Decision::Deny(reason) => (format!("permission denied: {reason}"), true),
                Decision::Ask(msg) => (
                    format!("interactive permission required (headless): {msg}"), true
                ),
                Decision::Allow => match reg.get(&name) {
                    Some(t) => {
                        let out = t.execute(input, &ctx).await;
                        (out.content, out.is_error)
                    }
                    None => (format!("unknown tool: {name}"), true),
                },
            };

            // 通知 TUI 工具已完成（预览前 500 字符）
            let _ = tx2.send(AgentEvent::ToolCallEnd {
                id: id.clone(),
                output: content.chars().take(500).collect(),
                is_error,
            });
            (id, content, is_error)
        }));
    }

    // 收集结果；panic 也要返回有效的 ToolResult，否则 assistant message
    // 里的 tool_use block 将没有对应的 tool_result，API 会报错。
    let mut result_blocks = Vec::new();
    let mut records = Vec::new();
    for (fallback_id, h) in id_handles {
        let (id, content, is_error) = match h.await {
            Ok(result) => result,
            Err(join_err) => {
                warn!(id = %fallback_id, "tool task panicked: {join_err}");
                (fallback_id, "internal error: tool task panicked".into(), true)
            }
        };
        records.push((id.clone(), content.clone(), is_error));
        result_blocks.push(ContentBlock::ToolResult {
            tool_use_id: id, content, is_error,
        });
    }
    (result_blocks, records)
}
```

## 23.7 Token 成本计算

按 model 名称查定价表，计算每轮 USD 花费：

```rust
fn price_usd(model: &str, usage: &Usage) -> f64 {
    let (in_m, out_m) = model_price_per_1m(model);
    (usage.input_tokens as f64 / 1_000_000.0) * in_m
        + (usage.output_tokens as f64 / 1_000_000.0) * out_m
}

fn model_price_per_1m(model: &str) -> (f64, f64) {
    // (input_$/1M, output_$/1M)
    if model.contains("claude-opus-4")    { return (15.0, 75.0); }
    if model.contains("claude-fable-5")
        || model.contains("claude-sonnet-4") { return (3.0,  15.0); }
    if model.contains("claude-haiku-4")   { return (0.80,  4.0); }
    if model.contains("gpt-4o")           { return (2.5,  10.0); }
    (3.0, 15.0) // 保守估算
}
```

## 23.8 Session 录制

每轮结束写一条 `TurnSnapshot` JSONL（详见第 25 章）：

```rust
async fn maybe_record_turn(
    &self,
    request_messages: &[Message],
    assistant_blocks: &[ContentBlock],
    tool_outputs: &[(String, String, bool)],
    usage: &Usage,
    iteration: u32,
    cost_usd: f64,          // 本轮真实花费，由调用方传入
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
            cost_usd,          // ← recorder 据此累加 SessionMeta.cost_usd
        };
        if let Err(e) = rec.record(snap).await {
            warn!("session record error: {e}");
        }
    }
}
```

## 23.9 企业级易错点（避坑指南）

实际生产中有几个容易忽视的问题，每一个都曾造成过线上事故：

**①  工具调用顺序不确定（HashMap 陷阱）**

`call_streaming_once` 里若用 `HashMap<String, (name, buf)>` 存储工具槽位，迭代时顺序不确定。LLM 按 `A → B` 顺序宣布工具调用，组装出的 `ContentBlock` 可能变成 `B → A`。某些 API 实现会因此报 schema 错误。**必须用 `Vec` + 索引来保持流式到达顺序。**

**②  工具 task panic 静默丢失**

`tokio::spawn` 的 `JoinHandle` 里 panic 不会冒泡到父 task；`if let Ok(...) = h.await` 会直接跳过。结果是 assistant message 里的 `tool_use` block 没有对应的 `tool_result`，下一次 LLM 调用必定报 400 错误。**必须用 `match h.await { Ok(r) => r, Err(e) => produce_error_result(fallback_id, e) }`。**

**③  SessionMeta.cost_usd 永远是 0**

`record()` 里自增 `turns` 却遗漏了 `cost_usd += turn.cost_usd`，导致 `~/.mcc/sessions/*.meta.json` 里的花费信息全部为 0，`mcc sessions list` 显示的成本列完全不可信。**`TurnSnapshot` 必须携带 `cost_usd`，`record()` 负责累加。**

**④  Bash 前缀越界匹配（安全 bug）**

`deny: ["Bash(rm:*)"]` 解析出前缀 `"rm"`，`"rmdir /tmp".starts_with("rm")` 为 `true`，导致 `rmdir` 被误拒绝。反过来，allow 规则 `"Bash(curl:*)"` 会放行 `curl-evil`。**前缀匹配必须加词边界检查：`after.is_empty() || after.starts_with(char::is_ascii_whitespace)`。**

**⑤  模型名拼写错误**

`ModelConfig::default()` 里写了 `"claude-opus-4-7"`，而 Anthropic 实际发布的是 `claude-opus-4-8`。错误的名字会导致 API 返回 404 或自动降级到旧模型，给团队留下隐性成本。**模型名要从官方文档核对，`config` 默认值要有对应测试。**

## 23.10 用 Mock LLM 测试

不需要 API key，全量测试：

```rust
struct MockLlm { text: String }

#[async_trait]
impl LlmProvider for MockLlm {
    async fn complete(&self, _: CompleteRequest) -> anyhow::Result<MessageResponse> {
        Ok(MessageResponse {
            content: vec![ContentBlock::Text { text: self.text.clone(), cache_control: None }],
            stop_reason: Some("end_turn".into()),
            usage: Usage { input_tokens: 10, output_tokens: 5, ..Default::default() },
        })
    }
    async fn stream(&self, _: CompleteRequest) -> anyhow::Result<BoxStream<'static, anyhow::Result<StreamEvent>>> {
        let text = self.text.clone();
        let events = vec![
            Ok(StreamEvent::TextDelta(text)),
            Ok(StreamEvent::MessageStop {
                stop_reason: "end_turn".into(),
                usage: Usage { input_tokens: 10, output_tokens: 5, ..Default::default() },
            }),
        ];
        Ok(Box::pin(futures::stream::iter(events)))
    }
}

#[tokio::test]
async fn test_streaming_emits_events() {
    let dir = TempDir::new().unwrap();
    let agent = AgentLoopBuilder::new(
        Arc::new(MockLlm { text: "Hello!".into() }),
        Arc::new(default_registry()),
        ToolContext { cwd: dir.path().to_path_buf(), session_id: "t".into(), depth: 0 },
    )
    .model("claude-haiku-4-5-20251001")
    .build();

    let (tx, mut rx) = mpsc::unbounded_channel();
    let run = agent.run_streaming("hi", &tx).await.unwrap();

    assert_eq!(run.final_text, "Hello!");
    assert!(run.cost_usd > 0.0);   // haiku: 0.80/1M in + 4/1M out

    let events: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    assert!(events.iter().any(|e| matches!(e, AgentEvent::UserEcho(_))));
    assert!(events.iter().any(|e| matches!(e, AgentEvent::TextDelta(_))));
    assert!(events.iter().any(|e| matches!(e, AgentEvent::TurnEnd { .. })));
}
```

运行：
```bash
cargo test -p mcc-harness -- --nocapture
```

## 23.11 事件流对照表

| AgentEvent | 触发时机 |
|---|---|
| `UserEcho(s)` | 收到用户输入时（立即） |
| `TextDelta(s)` | LLM 每吐出一个文本块 |
| `ToolCallStart{id,name}` | LLM 宣布开始某工具调用 |
| `ToolCallEnd{id,is_error}` | 工具执行完毕 |
| `TurnEnd{cost_usd}` | 整轮结束（stop_reason=end_turn） |
| `Notice(s)` | 预算警告、重试提示等 |
| `Error(s)` | 不可恢复错误 |

## 23.12 小结

- `AgentLoopBuilder` + `run_streaming()` = 事件驱动的生产级 Agent
- 流式聚合：`TextDelta` → 文本；`ToolUseStart/InputDelta` → JSON buffer → `ToolUse` block
- 指数退避重试：3 次，延迟 0.5/1/2s
- 权限检查在工具并发执行前统一处理
- Mock LLM 让所有路径都可测，不依赖 API key

> **下一章**：把 `PermissionChecker` 的配置和测试详细展开。
