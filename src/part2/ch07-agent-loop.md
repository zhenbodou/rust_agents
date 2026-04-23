# 第 7 章 Agent Loop：ReAct 与 Tool-calling 循环

> 目标：把前面所有零件拼成一个**真正跑起来的 Agent**，能自主完成多步任务。

## 7.1 两种主流循环范式

### ReAct (Reason + Act)

早期范式：模型在每步显式输出 `Thought:` / `Action:` / `Observation:`。现在被原生 tool_use 取代，但思想还在：**思考 → 行动 → 观察 → 再思考**。

### Tool-calling Loop（现代主流）

原生工具调用：模型用 `tool_use` 内容块直接发起调用，不需要手动解析文本。本书采用此范式。

```text
loop {
    response = llm.complete(messages, tools)
    messages.append(response)
    match response.stop_reason {
        "end_turn" | "stop_sequence" => break,
        "tool_use" => {
            for call in response.tool_calls() {
                result = tools[call.name].execute(call.input)
                messages.append(tool_result(call.id, result))
            }
        }
        "max_tokens" => { handle... }
    }
}
```

## 7.2 完整实现：`examples/07-agent-loop`

### 7.2.1 AgentLoop 结构

```rust
use std::sync::Arc;
use anyhow::{bail, Result};

pub struct AgentLoop {
    pub llm: Arc<dyn LlmProvider>,
    pub registry: Arc<ToolRegistry>,
    pub ctx: ToolContext,
    pub system: String,
    pub model: String,
    pub max_tokens: u32,
    pub max_iterations: u32,   // 防死循环
    pub temperature: f32,
}

pub struct AgentRun {
    pub final_text: String,
    pub messages: Vec<Message>,
    pub iterations: u32,
    pub total_usage: Usage,
}

impl AgentLoop {
    pub async fn run(&self, user_input: impl Into<String>) -> Result<AgentRun> {
        let mut messages = vec![Message::user(user_input)];
        let mut total = Usage::default();

        for iter in 1..=self.max_iterations {
            tracing::info!(iter, "loop iteration");

            let resp = self.llm.complete(CompleteRequest {
                model: self.model.clone(),
                max_tokens: self.max_tokens,
                messages: messages.clone(),
                system: Some(self.system.clone()),
                temperature: Some(self.temperature),
                tools: Some(self.registry.as_api_schema()),
            }).await?;

            total.input_tokens  += resp.usage.input_tokens;
            total.output_tokens += resp.usage.output_tokens;

            // 把 assistant 回复原样加入历史
            messages.push(Message { role: Role::Assistant, content: resp.content.clone() });

            let stop = resp.stop_reason.as_deref().unwrap_or("");
            match stop {
                "end_turn" | "stop_sequence" => {
                    let text = extract_text(&resp.content);
                    return Ok(AgentRun { final_text: text, messages, iterations: iter, total_usage: total });
                }
                "tool_use" => {
                    let tool_results = self.run_tool_calls(&resp.content).await;
                    messages.push(Message { role: Role::User, content: tool_results });
                }
                "max_tokens" => bail!("assistant hit max_tokens; raise limit or ask for shorter output"),
                other => bail!("unexpected stop_reason: {other}"),
            }
        }
        bail!("exceeded max_iterations={}", self.max_iterations)
    }

    async fn run_tool_calls(&self, blocks: &[ContentBlock]) -> Vec<ContentBlock> {
        let mut results = Vec::new();
        // 并行执行所有 tool_use（模型一轮可能请求多个）
        let calls: Vec<_> = blocks.iter().filter_map(|b| {
            if let ContentBlock::ToolUse { id, name, input } = b {
                Some((id.clone(), name.clone(), input.clone()))
            } else { None }
        }).collect();

        let mut futs = Vec::with_capacity(calls.len());
        for (id, name, input) in calls {
            let reg = self.registry.clone();
            let ctx = self.ctx.clone();
            futs.push(tokio::spawn(async move {
                let (content, is_error) = match reg.get(&name) {
                    Some(t) => {
                        let out = t.execute(input, &ctx).await;
                        (out.content, out.is_error)
                    }
                    None => (format!("unknown tool: {name}"), true),
                };
                ContentBlock::ToolResult { tool_use_id: id, content, is_error }
            }));
        }
        for f in futs {
            if let Ok(r) = f.await { results.push(r); }
        }
        results
    }
}

fn extract_text(blocks: &[ContentBlock]) -> String {
    blocks.iter().filter_map(|b| {
        if let ContentBlock::Text { text } = b { Some(text.as_str()) } else { None }
    }).collect::<Vec<_>>().join("\n")
}
```

**几个企业级要点**：

- **`max_iterations`** 防死循环（模型有时会 "ping-pong" 永远调用工具）
- **并行执行 tool_use**：一轮可能返回多个 tool call，用 `tokio::spawn` 并发执行
- **usage 累计**：后续做成本面板
- **所有分支都有明确错误**：不要 `unwrap`

### 7.2.2 让 `ToolContext` 可 clone

由于我们在并行 task 里用到了 ctx，需要：

```rust
#[derive(Clone)]
pub struct ToolContext {
    pub cwd: std::path::PathBuf,
    pub session_id: String,
}
```

### 7.2.3 跑起来

```rust
#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();

    let llm = Arc::new(AnthropicClient::from_env()?);
    let mut registry = ToolRegistry::default();
    registry.register(Arc::new(ReadFileTool));
    registry.register(Arc::new(ListDirTool));
    let registry = Arc::new(registry);

    let agent = AgentLoop {
        llm,
        registry,
        ctx: ToolContext { cwd: std::env::current_dir()?, session_id: uuid::Uuid::new_v4().to_string() },
        system: "你是一个仔细的项目分析助手。先用工具查看再回答。".into(),
        model: "claude-opus-4-7".into(),
        max_tokens: 2048,
        max_iterations: 8,
        temperature: 0.0,
    };

    let run = agent.run("分析当前项目的结构，告诉我这是什么项目、入口在哪、主要模块是什么。").await?;
    println!("\n=== FINAL ({} iterations, {} in / {} out tokens) ===\n{}",
        run.iterations, run.total_usage.input_tokens, run.total_usage.output_tokens, run.final_text);
    Ok(())
}
```

第一次运行你会看到模型主动调用 `list_dir`、`read_file`，几轮后写出分析结论。**你的第一个真正的 Agent 完成了。**

## 7.3 循环中的常见问题

### 7.3.1 "工具偏好症"

模型有时会反复调用同一个工具。对策：

- 在 system 里加 "如果已经有足够信息，停止使用工具并直接回答"
- 追踪工具调用历史，超过 N 次相同调用时在 tool_result 里追加提示

### 7.3.2 上下文溢出

多轮后 messages 会变得很长。基础对策（完整方案见第 10 章）：

- 定期压缩：让另一个 LLM 调用生成 "到目前为止的摘要"，替换早期消息
- 工具结果截断到 N KB

### 7.3.3 并发安全

多个工具并行写同一文件会冲突。对策：

- 每个可变资源一个 `tokio::sync::Mutex`
- 或者在 tool 层声明 `is_concurrent_safe()`，registry 串行执行不安全的

### 7.3.4 部分失败

一个 tool 失败时，不要终止整个循环。**把错误写进 tool_result（is_error=true），让模型自己决定是重试、换方法、还是放弃**。这是 Agent 稳健性的精髓——**让 LLM 处理 LLM 能处理的问题**。

## 7.4 Agent 的"停下来"的能力

一个成熟 Agent 必须知道何时停。信号包括：

- `stop_reason == end_turn` —— 模型主动结束
- 达到 `max_iterations`
- 达到 `budget_tokens`（成本上限）
- 用户通过中断信号取消（第 21 章 TUI 里讲）
- 外部 Hook 决定终止（第 12 章）

为此我们把 `AgentLoop` 加个 `Stopper`：

```rust
pub trait Stopper: Send + Sync {
    fn should_stop(&self, state: &LoopState) -> Option<StopReason>;
}
pub struct LoopState<'a> {
    pub iteration: u32,
    pub total_usage: Usage,
    pub elapsed: std::time::Duration,
    pub messages: &'a [Message],
}
pub enum StopReason { Budget, Timeout, External, MaxIterations }
```

这套接口是 Part 5 主循环的地基。

## 7.5 小结

- Tool-calling Loop 是现代 Agent 的核心控制流
- 并发执行 tool call + 错误写回 tool_result + 明确停机条件
- 你的 Agent 已经能自主读文件、列目录、写总结了

> **下一章**：让它拥有"记忆"——短期窗口、长期持久化、向量检索。

