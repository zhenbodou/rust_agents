//! 核心类型：Message / ContentBlock / Tool trait / ToolContext / AgentError。
//! 无外部 I/O 依赖。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("LLM API error: {0}")]
    Api(String),

    #[error("Tool `{name}` failed: {msg}")]
    Tool { name: String, msg: String },

    #[error("Permission denied: {0}")]
    Permission(String),

    #[error("Budget exceeded: {0}")]
    Budget(String),

    #[error("Cancelled")]
    Cancelled,

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, AgentError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        cache_control: Option<CacheControl>,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(default)]
        is_error: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheControl {
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

impl Message {
    pub fn user(s: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![ContentBlock::Text { text: s.into(), cache_control: None }],
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u32,
    #[serde(default)]
    pub output_tokens: u32,
    #[serde(default)]
    pub cache_creation_input_tokens: u32,
    #[serde(default)]
    pub cache_read_input_tokens: u32,
}

// ------------------ Tool trait ------------------

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> serde_json::Value;
    async fn execute(&self, input: serde_json::Value, ctx: &ToolContext) -> ToolOutput;
}

#[derive(Clone, Debug)]
pub struct ToolContext {
    pub cwd: PathBuf,
    pub session_id: String,
    pub depth: u32,
}

#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
}

impl ToolOutput {
    pub fn ok(s: impl Into<String>) -> Self {
        Self { content: s.into(), is_error: false }
    }
    pub fn err(s: impl Into<String>) -> Self {
        Self { content: s.into(), is_error: true }
    }
}

// ------------------ Event ------------------

#[derive(Debug, Clone)]
pub enum AgentEvent {
    UserEcho(String),
    TextDelta(String),
    ToolCallStart { id: String, name: String, args_preview: String },
    ToolCallEnd { id: String, output: String, is_error: bool },
    TurnEnd { cost_usd: f64 },
    Notice(String),
    Error(String),
    PermissionRequest { id: u64, message: String },
}
