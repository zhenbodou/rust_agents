//! mcc-llm: LlmProvider trait + Anthropic / OpenAI 实现 + auto 工厂。

use anyhow::Result;
use async_trait::async_trait;
use futures::stream::BoxStream;
use mcc_core::{Message, Usage};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct CompleteRequest {
    pub model: String,
    pub max_tokens: u32,
    pub messages: Vec<Message>,
    pub system: Option<String>,
    pub temperature: Option<f32>,
    pub tools: Option<serde_json::Value>,
}

#[derive(Debug)]
pub struct MessageResponse {
    pub content: Vec<mcc_core::ContentBlock>,
    pub stop_reason: Option<String>,
    pub usage: Usage,
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

pub mod anthropic;
pub mod openai;

/// 根据环境变量自动挑选 provider：
/// - 优先 `ANTHROPIC_API_KEY` → AnthropicClient
/// - 其次 `OPENAI_API_KEY` → OpenAiClient
///   （兼容 DeepSeek / Kimi / Qwen / Groq / vLLM / OpenRouter 等端点，
///    通过 `OPENAI_BASE_URL` 环境变量指定）
pub fn auto_provider_from_env() -> Result<Arc<dyn LlmProvider>> {
    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        Ok(Arc::new(anthropic::AnthropicClient::from_env()?))
    } else if std::env::var("OPENAI_API_KEY").is_ok() {
        Ok(Arc::new(openai::OpenAiClient::from_env()?))
    } else {
        anyhow::bail!(
            "No LLM API key found. Set ANTHROPIC_API_KEY or OPENAI_API_KEY \
             (and optionally OPENAI_BASE_URL for a compatible gateway)."
        )
    }
}
