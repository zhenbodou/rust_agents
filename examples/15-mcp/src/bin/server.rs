//! Demo MCP server: 暴露 `echo` 和 `add` 两个工具。
//!
//! 运行：`cargo run -p ex15-mcp --bin mcp-server-demo`

use anyhow::Result;
use async_trait::async_trait;
use ex15_mcp::server::{text_content, McpServer, McpTool};
use ex15_mcp::{CallToolResult, ToolInfo};
use serde_json::{json, Value};

struct EchoTool;

#[async_trait]
impl McpTool for EchoTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "echo".into(),
            description: "Return the input text unchanged.".into(),
            input_schema: json!({
                "type": "object",
                "required": ["text"],
                "properties": {"text": {"type": "string"}}
            }),
        }
    }
    async fn call(&self, args: Value) -> Result<CallToolResult> {
        let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("");
        Ok(text_content(text))
    }
}

struct AddTool;

#[async_trait]
impl McpTool for AddTool {
    fn info(&self) -> ToolInfo {
        ToolInfo {
            name: "add".into(),
            description: "Add two integers.".into(),
            input_schema: json!({
                "type": "object",
                "required": ["a", "b"],
                "properties": {"a": {"type": "integer"}, "b": {"type": "integer"}}
            }),
        }
    }
    async fn call(&self, args: Value) -> Result<CallToolResult> {
        let a = args.get("a").and_then(|v| v.as_i64()).unwrap_or(0);
        let b = args.get("b").and_then(|v| v.as_i64()).unwrap_or(0);
        Ok(text_content((a + b).to_string()))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // MCP servers 默认日志写 stderr，stdout 仅走协议。
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter("info")
        .init();

    let mut server = McpServer::new("demo-server", env!("CARGO_PKG_VERSION"));
    server.register(EchoTool);
    server.register(AddTool);
    server.serve_stdio().await
}
