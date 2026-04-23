//! OpenAI Chat Completions 客户端 —— 兼容 OpenAI 本体与所有 OpenAI-compatible
//! 端点（DeepSeek、Kimi、Qwen、Groq、OpenRouter、vLLM、SGLang、Ollama、…）。
//!
//! 只需设置：
//! ```bash
//! export OPENAI_API_KEY=...
//! export OPENAI_BASE_URL=https://api.deepseek.com/v1   # 可选
//! ```

use crate::{CompleteRequest, LlmProvider, MessageResponse, StreamEvent};
use anyhow::{anyhow, Context, Result};
use async_stream::try_stream;
use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures::{stream::BoxStream, StreamExt};
use mcc_core::{ContentBlock, Message, Role, Usage};
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashSet;

pub struct OpenAiClient {
    http: Client,
    api_key: String,
    base_url: String,
}

impl OpenAiClient {
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("OPENAI_API_KEY").context("OPENAI_API_KEY not set")?;
        let base_url = std::env::var("OPENAI_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1".into());
        Ok(Self {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()?,
            api_key,
            base_url,
        })
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }
}

#[async_trait]
impl LlmProvider for OpenAiClient {
    async fn complete(&self, req: CompleteRequest) -> Result<MessageResponse> {
        let body = build_body(&req, false);

        let resp = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("send request")?;

        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(anyhow!("OpenAI API {}: {}", status, text));
        }
        let v: Value = serde_json::from_str(&text)?;
        from_openai_response(v)
    }

    async fn stream(
        &self,
        req: CompleteRequest,
    ) -> Result<BoxStream<'static, Result<StreamEvent>>> {
        let body = build_body(&req, true);

        let resp = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?
            .error_for_status()?;

        let s = try_stream! {
            let mut events = resp.bytes_stream().eventsource();
            let mut seen_tools: HashSet<u64> = HashSet::new();
            let mut final_stop: Option<String> = None;
            let mut usage = Usage::default();

            while let Some(ev) = events.next().await {
                let ev = ev.map_err(|e| anyhow!(e))?;
                if ev.data.trim() == "[DONE]" { break; }

                let v: Value = match serde_json::from_str(&ev.data) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                if let Some(u) = parse_usage(&v) {
                    usage = merge_usage(usage, u);
                }

                let Some(choice) = v.get("choices").and_then(|c| c.get(0)) else { continue };
                let delta = choice.get("delta").cloned().unwrap_or(Value::Null);

                if let Some(text) = delta.get("content").and_then(|v| v.as_str()) {
                    if !text.is_empty() {
                        yield StreamEvent::TextDelta(text.to_string());
                    }
                }

                if let Some(tcs) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                    for tc in tcs {
                        let index = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
                        if !seen_tools.contains(&index) {
                            if let (Some(id), Some(name)) = (
                                tc.get("id").and_then(|v| v.as_str()),
                                tc.pointer("/function/name").and_then(|v| v.as_str()),
                            ) {
                                seen_tools.insert(index);
                                yield StreamEvent::ToolUseStart {
                                    id: id.to_string(),
                                    name: name.to_string(),
                                };
                            }
                        }
                        if let Some(arg) =
                            tc.pointer("/function/arguments").and_then(|v| v.as_str())
                        {
                            if !arg.is_empty() {
                                yield StreamEvent::ToolUseInputDelta(arg.to_string());
                            }
                        }
                    }
                }

                if let Some(finish) = choice.get("finish_reason").and_then(|v| v.as_str()) {
                    final_stop = Some(map_finish_reason(finish).to_string());
                }
            }

            yield StreamEvent::MessageStop {
                stop_reason: final_stop.unwrap_or_else(|| "end_turn".into()),
                usage,
            };
        };

        Ok(Box::pin(s))
    }
}

fn build_body(req: &CompleteRequest, stream: bool) -> Value {
    let mut body = json!({
        "model": req.model,
        "messages": to_openai_messages(req.system.as_deref(), &req.messages),
        "max_tokens": req.max_tokens,
    });
    if let Some(t) = req.temperature {
        body["temperature"] = json!(t);
    }
    if let Some(tools) = to_openai_tools(req.tools.as_ref()) {
        body["tools"] = tools;
    }
    if stream {
        body["stream"] = json!(true);
        body["stream_options"] = json!({"include_usage": true});
    }
    body
}

fn to_openai_messages(system: Option<&str>, msgs: &[Message]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    if let Some(s) = system {
        if !s.is_empty() {
            out.push(json!({"role": "system", "content": s}));
        }
    }
    for m in msgs {
        match m.role {
            Role::System => {
                if let Some(ContentBlock::Text { text, .. }) = m.content.first() {
                    out.push(json!({"role": "system", "content": text}));
                }
            }
            Role::User => {
                let mut text_buf = String::new();
                let mut tool_msgs: Vec<Value> = Vec::new();
                for b in &m.content {
                    match b {
                        ContentBlock::Text { text, .. } => {
                            if !text_buf.is_empty() {
                                text_buf.push('\n');
                            }
                            text_buf.push_str(text);
                        }
                        ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                            ..
                        } => {
                            tool_msgs.push(json!({
                                "role": "tool",
                                "tool_call_id": tool_use_id,
                                "content": content,
                            }));
                        }
                        _ => {}
                    }
                }
                if !text_buf.is_empty() {
                    out.push(json!({"role": "user", "content": text_buf}));
                }
                out.extend(tool_msgs);
            }
            Role::Assistant => {
                let mut text_buf = String::new();
                let mut tool_calls: Vec<Value> = Vec::new();
                for b in &m.content {
                    match b {
                        ContentBlock::Text { text, .. } => {
                            if !text_buf.is_empty() {
                                text_buf.push('\n');
                            }
                            text_buf.push_str(text);
                        }
                        ContentBlock::ToolUse { id, name, input } => {
                            tool_calls.push(json!({
                                "id": id,
                                "type": "function",
                                "function": {
                                    "name": name,
                                    "arguments": input.to_string(),
                                }
                            }));
                        }
                        _ => {}
                    }
                }
                let mut msg = serde_json::Map::new();
                msg.insert("role".into(), json!("assistant"));
                msg.insert(
                    "content".into(),
                    if text_buf.is_empty() {
                        Value::Null
                    } else {
                        json!(text_buf)
                    },
                );
                if !tool_calls.is_empty() {
                    msg.insert("tool_calls".into(), Value::Array(tool_calls));
                }
                out.push(Value::Object(msg));
            }
        }
    }
    out
}

fn to_openai_tools(tools: Option<&Value>) -> Option<Value> {
    let arr = tools?.as_array()?;
    Some(Value::Array(
        arr.iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.get("name").cloned().unwrap_or(Value::Null),
                        "description": t.get("description").cloned().unwrap_or(Value::Null),
                        "parameters": t
                            .get("input_schema")
                            .cloned()
                            .unwrap_or(json!({"type": "object"})),
                    }
                })
            })
            .collect(),
    ))
}

fn from_openai_response(v: Value) -> Result<MessageResponse> {
    let choice = v
        .get("choices")
        .and_then(|c| c.get(0))
        .ok_or_else(|| anyhow!("no choices"))?;
    let message = choice
        .get("message")
        .ok_or_else(|| anyhow!("no message"))?;
    let finish = choice
        .get("finish_reason")
        .and_then(|v| v.as_str())
        .unwrap_or("stop");

    let mut content: Vec<ContentBlock> = Vec::new();
    if let Some(text) = message.get("content").and_then(|v| v.as_str()) {
        if !text.is_empty() {
            content.push(ContentBlock::Text {
                text: text.to_string(),
                cache_control: None,
            });
        }
    }
    if let Some(arr) = message.get("tool_calls").and_then(|v| v.as_array()) {
        for tc in arr {
            let id = tc
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let name = tc
                .pointer("/function/name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let args_str = tc
                .pointer("/function/arguments")
                .and_then(|v| v.as_str())
                .unwrap_or("{}");
            let input: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
            content.push(ContentBlock::ToolUse { id, name, input });
        }
    }

    Ok(MessageResponse {
        content,
        stop_reason: Some(map_finish_reason(finish).to_string()),
        usage: parse_usage(&v).unwrap_or_default(),
    })
}

fn map_finish_reason(f: &str) -> &str {
    match f {
        "stop" => "end_turn",
        "tool_calls" | "function_call" => "tool_use",
        "length" => "max_tokens",
        "content_filter" => "stop_sequence",
        other => other,
    }
}

fn parse_usage(v: &Value) -> Option<Usage> {
    let u = v.get("usage")?;
    if !u.is_object() {
        return None;
    }
    Some(Usage {
        input_tokens: u.get("prompt_tokens").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
        output_tokens: u
            .get("completion_tokens")
            .and_then(|x| x.as_u64())
            .unwrap_or(0) as u32,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: u
            .pointer("/prompt_tokens_details/cached_tokens")
            .and_then(|x| x.as_u64())
            .unwrap_or(0) as u32,
    })
}

fn merge_usage(a: Usage, b: Usage) -> Usage {
    Usage {
        input_tokens: a.input_tokens.max(b.input_tokens),
        output_tokens: a.output_tokens.max(b.output_tokens),
        cache_creation_input_tokens: a
            .cache_creation_input_tokens
            .max(b.cache_creation_input_tokens),
        cache_read_input_tokens: a.cache_read_input_tokens.max(b.cache_read_input_tokens),
    }
}
