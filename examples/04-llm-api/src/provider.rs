use crate::types::*;
use async_trait::async_trait;
use anyhow::Result;
use futures::stream::BoxStream;

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

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, req: CompleteRequest) -> Result<MessageResponse>;

    async fn stream(
        &self,
        req: CompleteRequest,
    ) -> Result<BoxStream<'static, Result<StreamEvent>>>;
}
