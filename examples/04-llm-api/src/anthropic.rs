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

    async fn stream(
        &self,
        req: CompleteRequest,
    ) -> Result<BoxStream<'static, Result<StreamEvent>>> {
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
            let ev = match ev {
                Ok(e) => e,
                Err(e) => return Some(Err(anyhow!(e))),
            };
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
                let name = v
                    .pointer("/content_block/name")
                    .and_then(|t| t.as_str())
                    .unwrap_or("");
                return Ok(Some(StreamEvent::ToolUseStart {
                    id: id.into(),
                    name: name.into(),
                }));
            }
            Ok(None)
        }
        "message_delta" => {
            let stop_reason = v
                .pointer("/delta/stop_reason")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            let usage = serde_json::from_value(v["usage"].clone()).unwrap_or_default();
            Ok(Some(StreamEvent::MessageStop { stop_reason, usage }))
        }
        _ => Ok(None),
    }
}
