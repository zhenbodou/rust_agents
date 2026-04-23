//! Model Context Protocol (MCP) —— 最小可用 client + server 实现。
//!
//! MCP 是 Anthropic 开源的标准化协议，让 LLM 宿主（Claude Code / Cursor / IDE 插件）
//! 能以统一方式连接外部工具与数据源。它基于 JSON-RPC 2.0 over stdio / HTTP / SSE。
//!
//! 本 crate 实现：
//! - stdio transport
//! - initialize / tools/list / tools/call / shutdown 核心 RPC
//! - 服务端暴露工具，客户端调用
//!
//! 生产环境请优先采用官方的 `mcp-sdk` 一旦成熟稳定。此处手写是为了理解协议。

pub mod protocol;
pub mod server;
pub mod client;

pub use protocol::*;
