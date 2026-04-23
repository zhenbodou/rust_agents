# 第 4 章 调用 LLM API（Anthropic / OpenAI）

> 目标：写出本书第一个"连通外网"的 Rust 程序，并建立一个**可扩展**的 LLM 客户端抽象，贯穿全书。

## 4.1 API 本质：一次 HTTP POST

忘掉所有花哨 SDK。**LLM API = 发一个 JSON 到一个 URL**。

### Anthropic Messages API

```http
POST https://api.anthropic.com/v1/messages
x-api-key: sk-ant-...
anthropic-version: 2023-06-01
content-type: application/json

{
  "model": "claude-opus-4-7",
  "max_tokens": 1024,
  "messages": [
    {"role": "user", "content": "Hello!"}
  ]
}
```

返回：

```json
{
  "id": "msg_01...",
  "type": "message",
  "role": "assistant",
  "content": [{"type": "text", "text": "Hello! How can I help?"}],
  "stop_reason": "end_turn",
  "usage": {"input_tokens": 8, "output_tokens": 10}
}
```

理解了这个结构，你就理解了 80% 的 "LLM SDK"。

## 4.2 示例工程：`examples/04-llm-api`

### `Cargo.toml`

```toml
[package]
name = "ex04-llm-api"
edition.workspace = true
version.workspace = true

[dependencies]
tokio.workspace = true
reqwest.workspace = true
serde.workspace = true
serde_json.workspace = true
anyhow.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
dotenvy.workspace = true
async-trait.workspace = true
eventsource-stream.workspace = true
futures.workspace = true
```

### 4.2.1 定义消息与请求结构

`src/types.rs`：

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    ToolUse { id: String, name: String, input: serde_json::Value },
    ToolResult { tool_use_id: String, content: String, #[serde(default)] is_error: bool },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

impl Message {
    pub fn user(text: impl Into<String>) -> Self {
        Self { role: Role::User, content: vec![ContentBlock::Text { text: text.into() }] }
    }
    pub fn assistant_text(text: impl Into<String>) -> Self {
        Self { role: Role::Assistant, content: vec![ContentBlock::Text { text: text.into() }] }
    }
}

#[derive(Debug, Serialize)]
pub struct CreateMessageRequest<'a> {
    pub model: &'a str,
    pub max_tokens: u32,
    pub messages: &'a [Message],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<&'a serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct MessageResponse {
    pub id: String,
    pub role: Role,
    pub content: Vec<ContentBlock>,
    pub stop_reason: Option<String>,
    pub usage: Usage,
}

#[derive(Debug, Deserialize, Clone, Copy, Default)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(default)]
    pub cache_creation_input_tokens: u32,
    #[serde(default)]
    pub cache_read_input_tokens: u32,
}
```

**关键设计点**：`ContentBlock` 是一个 tagged enum，内含 `Text / ToolUse / ToolResult`——这就是 Anthropic 多模态内容块的 Rust 翻译。后面所有章节都基于这个结构。

### 4.2.2 抽象 `LlmProvider` Trait

为什么要 trait？—— **解耦 Anthropic / OpenAI / 国产模型**。企业级项目通常要同时支持多家，或做 A/B 测试。

`src/provider.rs`：

```rust
use crate::types::*;
use async_trait::async_trait;
use anyhow::Result;
use futures::stream::BoxStream;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// 非流式一次性返回
    async fn complete(&self, req: CompleteRequest) -> Result<MessageResponse>;

    /// 流式返回（第 23 章主循环用）
    async fn stream(&self, req: CompleteRequest) -> Result<BoxStream<'static, Result<StreamEvent>>>;
}

#[derive(Debug, Clone)]
pub struct CompleteRequest {
    pub model: String,
    pub max_tokens: u32,
    pub messages: Vec<Message>,
    pub system: Option<String>,
    pub temperature: Option<f32>,
    pub tools: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    TextDelta(String),
    ToolUseStart { id: String, name: String },
    ToolUseInputDelta(String),
    MessageStop { stop_reason: String, usage: Usage },
}
```

### 4.2.3 实现 Anthropic 客户端

`src/anthropic.rs`：

```rust
use crate::{provider::*, types::*};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use futures::{stream::BoxStream, StreamExt};
use reqwest::Client;
use serde_json::json;

pub struct AnthropicClient {
    http: Client,
    api_key: String,
    base_url: String,
}

impl AnthropicClient {
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .context("ANTHROPIC_API_KEY not set")?;
        Ok(Self {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()?,
            api_key,
            base_url: "https://api.anthropic.com".into(),
        })
    }
}

#[async_trait]
impl LlmProvider for AnthropicClient {
    async fn complete(&self, req: CompleteRequest) -> Result<MessageResponse> {
        let body = json!({
            "model": req.model,
            "max_tokens": req.max_tokens,
            "messages": req.messages,
            "system": req.system,
            "temperature": req.temperature,
            "tools": req.tools,
        });

        let resp = self.http
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .context("send request")?;

        let status = resp.status();
        let text = resp.text().await?;

        if !status.is_success() {
            return Err(anyhow!("Anthropic API {}: {}", status, text));
        }

        serde_json::from_str(&text).context("parse response")
    }

    async fn stream(&self, req: CompleteRequest) -> Result<BoxStream<'static, Result<StreamEvent>>> {
        use eventsource_stream::Eventsource;

        let body = json!({
            "model": req.model,
            "max_tokens": req.max_tokens,
            "messages": req.messages,
            "system": req.system,
            "temperature": req.temperature,
            "tools": req.tools,
            "stream": true,
        });

        let resp = self.http
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await?
            .error_for_status()?;

        let stream = resp.bytes_stream().eventsource().filter_map(|ev| async move {
            let ev = match ev { Ok(e) => e, Err(e) => return Some(Err(anyhow!(e))) };
            parse_sse_event(&ev.event, &ev.data).transpose()
        });

        Ok(stream.boxed())
    }
}

fn parse_sse_event(event: &str, data: &str) -> Result<Option<StreamEvent>> {
    let v: serde_json::Value = serde_json::from_str(data)?;
    match event {
        "content_block_delta" => {
            if let Some(text) = v.pointer("/delta/text").and_then(|t| t.as_str()) {
                return Ok(Some(StreamEvent::TextDelta(text.into())));
            }
            if let Some(p) = v.pointer("/delta/partial_json").and_then(|t| t.as_str()) {
                return Ok(Some(StreamEvent::ToolUseInputDelta(p.into())));
            }
            Ok(None)
        }
        "content_block_start" => {
            if let Some(id) = v.pointer("/content_block/id").and_then(|t| t.as_str()) {
                let name = v.pointer("/content_block/name").and_then(|t| t.as_str()).unwrap_or("");
                return Ok(Some(StreamEvent::ToolUseStart { id: id.into(), name: name.into() }));
            }
            Ok(None)
        }
        "message_delta" => {
            let stop_reason = v.pointer("/delta/stop_reason").and_then(|t| t.as_str()).unwrap_or("").to_string();
            let usage = serde_json::from_value(v["usage"].clone()).unwrap_or_default();
            Ok(Some(StreamEvent::MessageStop { stop_reason, usage }))
        }
        _ => Ok(None),
    }
}
```

### 4.2.4 主程序

`src/main.rs`：

```rust
mod types;
mod provider;
mod anthropic;

use anyhow::Result;
use anthropic::AnthropicClient;
use provider::{CompleteRequest, LlmProvider, StreamEvent};
use types::Message;
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let client = AnthropicClient::from_env()?;

    // ---- 非流式 ----
    let resp = client.complete(CompleteRequest {
        model: "claude-opus-4-7".into(),
        max_tokens: 256,
        messages: vec![Message::user("用一句话介绍 AI Agent。")],
        system: Some("你是一位简洁的技术讲师。".into()),
        temperature: Some(0.2),
        tools: None,
    }).await?;

    for block in &resp.content {
        if let types::ContentBlock::Text { text } = block {
            println!("[assistant] {text}");
        }
    }
    println!("usage: {:?}", resp.usage);

    // ---- 流式 ----
    println!("\n--- streaming ---");
    let mut stream = client.stream(CompleteRequest {
        model: "claude-opus-4-7".into(),
        max_tokens: 256,
        messages: vec![Message::user("数到 5，每个数字换一行。")],
        system: None,
        temperature: Some(0.0),
        tools: None,
    }).await?;

    while let Some(event) = stream.next().await {
        match event? {
            StreamEvent::TextDelta(t) => {
                use std::io::Write;
                print!("{t}");
                std::io::stdout().flush()?;
            }
            StreamEvent::MessageStop { stop_reason, usage } => {
                println!("\n[stop={stop_reason}, usage={usage:?}]");
            }
            _ => {}
        }
    }
    Ok(())
}
```

运行：

```bash
cargo run -p ex04-llm-api
```

## 4.3 OpenAI 兼容适配

国内很多平台（DeepSeek、Kimi、通义、Groq、OpenRouter、vLLM、SGLang、Ollama）都提供 **OpenAI 兼容** endpoint。我们为 crate 加一个 `OpenAiClient`，实现同一 `LlmProvider` trait，**主 Agent 代码无需任何改动**即可切换。

### 4.3.1 关键差异

| 点 | Anthropic | OpenAI |
|---|---|---|
| 端点 | `POST /v1/messages` | `POST /v1/chat/completions` |
| 鉴权 | `x-api-key` | `Authorization: Bearer` |
| system | 顶层字段 | 第一条 `role:"system"` 消息 |
| 工具 schema | `{name, description, input_schema}` | 包在 `{type:"function", function:{...parameters:...}}` |
| 工具调用 | `ContentBlock::ToolUse` | `message.tool_calls[]` |
| 工具结果 | 下一轮 `ContentBlock::ToolResult` | 独立的 `role:"tool"` 消息 |
| stop_reason | `end_turn`/`tool_use`/`max_tokens` | `stop`/`tool_calls`/`length` |
| 流式事件 | 多类型事件（`content_block_start` 等） | 统一 `delta`，tool 参数分片 `function.arguments` |
| 流式 usage | 每帧都有 | 需设 `stream_options.include_usage=true` |

### 4.3.2 消息双向翻译（核心）

我们的统一 `Message` 可能包含混合块（文本 + tool_use + tool_result）。翻译到 OpenAI 要做两件事：

1. 把 **assistant 消息**的 `ContentBlock::ToolUse` 聚合到 `tool_calls[]`
2. 把 **user 消息**的每个 `ContentBlock::ToolResult` **拆出**成独立的 `role:"tool"` 消息

关键片段（完整代码见 [examples/04-llm-api/src/openai.rs](../../../examples/04-llm-api/src/openai.rs)）：

```rust
fn to_openai_messages(system: Option<&str>, msgs: &[Message]) -> Vec<Value> {
    let mut out = Vec::new();
    if let Some(s) = system { out.push(json!({"role":"system","content":s})); }

    for m in msgs {
        match m.role {
            Role::Assistant => {
                let mut text_buf = String::new();
                let mut tool_calls = Vec::new();
                for b in &m.content {
                    match b {
                        ContentBlock::Text { text, .. } => text_buf.push_str(text),
                        ContentBlock::ToolUse { id, name, input } => {
                            tool_calls.push(json!({
                                "id": id, "type": "function",
                                "function": {"name": name, "arguments": input.to_string()}
                            }));
                        }
                        _ => {}
                    }
                }
                out.push(json!({
                    "role": "assistant",
                    "content": if text_buf.is_empty() { Value::Null } else { json!(text_buf) },
                    "tool_calls": if tool_calls.is_empty() { Value::Null } else { json!(tool_calls) },
                }));
            }
            Role::User => {
                // 拆分：文本作为一条 user 消息，每个 ToolResult 作为独立 role="tool" 消息
                let mut text_buf = String::new();
                let mut tool_msgs = Vec::new();
                for b in &m.content {
                    match b {
                        ContentBlock::Text { text, .. } => text_buf.push_str(text),
                        ContentBlock::ToolResult { tool_use_id, content, .. } => {
                            tool_msgs.push(json!({
                                "role": "tool", "tool_call_id": tool_use_id, "content": content
                            }));
                        }
                        _ => {}
                    }
                }
                if !text_buf.is_empty() { out.push(json!({"role":"user","content":text_buf})); }
                out.extend(tool_msgs);
            }
            _ => {}
        }
    }
    out
}
```

### 4.3.3 stop_reason 归一化

```rust
fn map_finish_reason(f: &str) -> &str {
    match f {
        "stop" => "end_turn",
        "tool_calls" | "function_call" => "tool_use",
        "length" => "max_tokens",
        "content_filter" => "stop_sequence",
        other => other,
    }
}
```

这样我们的主循环**不用关心**底层用的是哪家 API —— 第 7 章的 `AgentLoop` 只看归一化后的 `stop_reason`。

### 4.3.4 流式 tool_call 的累积

OpenAI 的 SSE 会**分片发送 tool_call 参数**：

```
data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"add","arguments":""}}]}}]}
data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"a\":"}}]}}]}
data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"1,\"b\":2}"}}]}}]}
data: {"choices":[{"finish_reason":"tool_calls"}]}
data: [DONE]
```

客户端要：**第一次见到某 `index` 时**发 `ToolUseStart(id, name)`，后续 `arguments` 片段发 `ToolUseInputDelta`。这就是下面这段逻辑（完整见 openai.rs）：

```rust
let mut seen_tools: HashSet<u64> = HashSet::new();
for tc in tcs {
    let index = tc["index"].as_u64().unwrap_or(0);
    if !seen_tools.contains(&index) {
        if let (Some(id), Some(name)) = (tc["id"].as_str(), tc["function"]["name"].as_str()) {
            seen_tools.insert(index);
            yield StreamEvent::ToolUseStart { id: id.into(), name: name.into() };
        }
    }
    if let Some(arg) = tc["function"]["arguments"].as_str() {
        if !arg.is_empty() { yield StreamEvent::ToolUseInputDelta(arg.into()); }
    }
}
```

### 4.3.5 Auto provider 工厂

让项目**根据环境变量**自动选：

```rust
pub fn auto_provider_from_env() -> Result<Arc<dyn LlmProvider>> {
    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        Ok(Arc::new(AnthropicClient::from_env()?))
    } else if std::env::var("OPENAI_API_KEY").is_ok() {
        Ok(Arc::new(OpenAiClient::from_env()?))
    } else {
        anyhow::bail!("No LLM API key found")
    }
}
```

### 4.3.6 切换到国内 / 自托管模型

```bash
# DeepSeek
export OPENAI_API_KEY=sk-xxx
export OPENAI_BASE_URL=https://api.deepseek.com/v1
export MODEL=deepseek-chat
cargo run -p ex04-llm-api

# Kimi (Moonshot)
export OPENAI_API_KEY=sk-xxx
export OPENAI_BASE_URL=https://api.moonshot.cn/v1
export MODEL=moonshot-v1-32k

# 本地 vLLM / SGLang
export OPENAI_API_KEY=dummy
export OPENAI_BASE_URL=http://localhost:8000/v1
export MODEL=Qwen3-32B
```

**整个 Agent Runtime 代码零改动。** 这就是第 4 章坚持做 `LlmProvider` trait 抽象的回报。

## 4.4 常见坑

| 症状 | 原因 | 解决 |
|---|---|---|
| 401 unauthorized | key 没加载 | 检查 `.env`，`dotenvy::dotenv()` 必须在 `env::var` 之前 |
| 400 max_tokens 超限 | 超过模型上限 | Opus/Sonnet 当前 8192（beta 更大） |
| 长请求超时 | 默认 reqwest 30s | 显式设 `timeout(120s)` |
| 流式事件丢失 | 没处理 `content_block_start` | 第 6 章讲 Tool Use 时你会明白为什么 |

## 4.5 小结

- 一次 LLM 调用 = 一次 HTTP POST，不要被 SDK 神秘化
- 用 trait 抽象 Provider，企业级必备
- 流式是第 21 章 TUI 实时输出的基础

> **下一章**：Prompt 工程与结构化输出——让 LLM 稳定产出机器能解析的格式。

