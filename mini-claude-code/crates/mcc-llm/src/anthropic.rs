//! Anthropic Messages API 客户端。支持非流式与 SSE 流式。

use crate::{CompleteRequest, LlmProvider, MessageResponse, StreamEvent};
use anyhow::{anyhow, Context, Result};
use async_stream::try_stream;
use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures::{stream::BoxStream, StreamExt};
use mcc_core::{ContentBlock, Usage};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};

pub struct AnthropicClient {
    http: Client,
    api_key: String,
    base_url: String,
}

#[derive(Debug, Deserialize)]
struct RawResponse {
    content: Vec<ContentBlock>,
    stop_reason: Option<String>,
    #[serde(default)]
    usage: Usage,
}

impl AnthropicClient {
    pub fn from_env() -> Result<Self> {
        let api_key =
            std::env::var("ANTHROPIC_API_KEY").context("ANTHROPIC_API_KEY not set")?;
        Ok(Self {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()?,
            api_key,
            base_url: std::env::var("ANTHROPIC_BASE_URL")
                .unwrap_or_else(|_| "https://api.anthropic.com".into()),
        })
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }
}

#[async_trait]
impl LlmProvider for AnthropicClient {
    async fn complete(&self, req: CompleteRequest) -> Result<MessageResponse> {
        let body = build_body(&req, false);

        let resp = self
            .http
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            return Err(anyhow!("Anthropic API {}: {}", status, text));
        }
        let raw: RawResponse = serde_json::from_str(&text)?;
        Ok(MessageResponse {
            content: raw.content,
            stop_reason: raw.stop_reason,
            usage: raw.usage,
        })
    }

    async fn stream(
        &self,
        req: CompleteRequest,
    ) -> Result<BoxStream<'static, Result<StreamEvent>>> {
        let body = build_body(&req, true);

        let resp = self
            .http
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await?
            .error_for_status()?;

        let s = try_stream! {
            let mut events = resp.bytes_stream().eventsource();
            let mut stop_reason = String::from("end_turn");
            let mut usage = Usage::default();

            while let Some(ev) = events.next().await {
                let ev = ev.map_err(|e| anyhow!(e))?;
                let data: Value = match serde_json::from_str(&ev.data) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                match ev.event.as_str() {
                    "message_start" => {
                        // usage.input_tokens / cache tokens 在此事件里有初值
                        if let Some(u) = data.pointer("/message/usage") {
                            let parsed: Usage =
                                serde_json::from_value(u.clone()).unwrap_or_default();
                            usage.input_tokens = parsed.input_tokens.max(usage.input_tokens);
                            usage.cache_creation_input_tokens = parsed
                                .cache_creation_input_tokens
                                .max(usage.cache_creation_input_tokens);
                            usage.cache_read_input_tokens = parsed
                                .cache_read_input_tokens
                                .max(usage.cache_read_input_tokens);
                        }
                    }
                    "content_block_start" => {
                        if let Some(cb) = data.get("content_block") {
                            if cb.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                                let id = cb.get("id").and_then(|v| v.as_str()).unwrap_or("");
                                let name = cb.get("name").and_then(|v| v.as_str()).unwrap_or("");
                                yield StreamEvent::ToolUseStart {
                                    id: id.to_string(),
                                    name: name.to_string(),
                                };
                            }
                        }
                    }
                    "content_block_delta" => {
                        let delta = data.get("delta").cloned().unwrap_or(Value::Null);
                        match delta.get("type").and_then(|t| t.as_str()) {
                            Some("text_delta") => {
                                if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                                    if !text.is_empty() {
                                        yield StreamEvent::TextDelta(text.to_string());
                                    }
                                }
                            }
                            Some("input_json_delta") => {
                                if let Some(p) =
                                    delta.get("partial_json").and_then(|v| v.as_str())
                                {
                                    yield StreamEvent::ToolUseInputDelta(p.to_string());
                                }
                            }
                            _ => {}
                        }
                    }
                    "message_delta" => {
                        if let Some(sr) =
                            data.pointer("/delta/stop_reason").and_then(|v| v.as_str())
                        {
                            stop_reason = sr.to_string();
                        }
                        if let Some(u) = data.get("usage") {
                            let parsed: Usage =
                                serde_json::from_value(u.clone()).unwrap_or_default();
                            usage.output_tokens = parsed.output_tokens.max(usage.output_tokens);
                        }
                    }
                    "message_stop" => {
                        // final signal
                    }
                    _ => {}
                }
            }

            yield StreamEvent::MessageStop { stop_reason, usage };
        };

        Ok(Box::pin(s))
    }
}

fn build_body(req: &CompleteRequest, stream: bool) -> Value {
    let mut body = json!({
        "model": req.model,
        "max_tokens": req.max_tokens,
        "messages": req.messages,
    });
    if let Some(s) = &req.system {
        body["system"] = json!(s);
    }
    if let Some(t) = req.temperature {
        body["temperature"] = json!(t);
    }
    if let Some(tools) = &req.tools {
        body["tools"] = tools.clone();
    }
    if stream {
        body["stream"] = json!(true);
    }
    body
}
