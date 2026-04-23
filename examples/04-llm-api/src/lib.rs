//! 第 4 章配套 crate：LLM API 客户端抽象 + Anthropic / OpenAI 兼容实现。
//! 作为可复用 lib 被第 5–8 章的示例 crate 引用。

pub mod types;
pub mod provider;
pub mod anthropic;
pub mod openai;

pub use types::*;
pub use provider::*;
pub use anthropic::AnthropicClient;
pub use openai::OpenAiClient;

use anyhow::Result;
use std::sync::Arc;

/// 根据环境变量自动挑选 provider：
/// - 优先 `ANTHROPIC_API_KEY`
/// - 其次 `OPENAI_API_KEY`（兼容 DeepSeek / Kimi / Qwen / Groq / vLLM 等）
pub fn auto_provider_from_env() -> Result<Arc<dyn LlmProvider>> {
    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        Ok(Arc::new(AnthropicClient::from_env()?))
    } else if std::env::var("OPENAI_API_KEY").is_ok() {
        Ok(Arc::new(OpenAiClient::from_env()?))
    } else {
        anyhow::bail!(
            "No LLM API key found. Set ANTHROPIC_API_KEY or OPENAI_API_KEY \
             (and optionally OPENAI_BASE_URL for a compatible gateway)."
        )
    }
}
