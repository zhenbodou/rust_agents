//! MCP server: 通过 stdio 暴露工具。

use crate::protocol::*;
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[async_trait]
pub trait McpTool: Send + Sync {
    fn info(&self) -> ToolInfo;
    async fn call(&self, args: serde_json::Value) -> Result<CallToolResult>;
}

pub struct McpServer {
    pub info: Implementation,
    pub tools: HashMap<String, Arc<dyn McpTool>>,
}

impl McpServer {
    pub fn new(name: &str, version: &str) -> Self {
        Self {
            info: Implementation { name: name.into(), version: version.into() },
            tools: HashMap::new(),
        }
    }

    pub fn register<T: McpTool + 'static>(&mut self, tool: T) {
        let info = tool.info();
        self.tools.insert(info.name.clone(), Arc::new(tool));
    }

    /// Serve over stdio (blocking on the current task).
    pub async fn serve_stdio(&self) -> Result<()> {
        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut reader = BufReader::new(stdin).lines();

        while let Some(line) = reader.next_line().await? {
            if line.trim().is_empty() { continue; }
            let req: JsonRpcRequest = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("invalid jsonrpc line: {e}");
                    continue;
                }
            };
            let resp = self.handle(req).await;
            if let Some(resp) = resp {
                let s = serde_json::to_string(&resp)? + "\n";
                stdout.write_all(s.as_bytes()).await?;
                stdout.flush().await?;
            }
        }
        Ok(())
    }

    async fn handle(&self, req: JsonRpcRequest) -> Option<JsonRpcResponse> {
        // Notifications (no id) don't expect a response.
        let Some(id) = req.id.clone() else {
            return None;
        };

        let result = match req.method.as_str() {
            "initialize" => {
                let caps = ServerCapabilities {
                    tools: Some(ToolsCapability { list_changed: false }),
                    ..Default::default()
                };
                serde_json::to_value(InitializeResult {
                    protocol_version: MCP_PROTOCOL_VERSION.into(),
                    capabilities: caps,
                    server_info: self.info.clone(),
                })
                .map_err(|e| to_err(-32603, format!("serialize: {e}")))
            }
            "tools/list" => {
                let tools = self.tools.values().map(|t| t.info()).collect();
                serde_json::to_value(ListToolsResult { tools })
                    .map_err(|e| to_err(-32603, format!("serialize: {e}")))
            }
            "tools/call" => {
                let params: CallToolParams = match req
                    .params
                    .and_then(|v| serde_json::from_value(v).ok())
                {
                    Some(p) => p,
                    None => return Some(err_resp(id, -32602, "invalid params")),
                };
                match self.tools.get(&params.name) {
                    Some(tool) => match tool.call(params.arguments).await {
                        Ok(r) => serde_json::to_value(r)
                            .map_err(|e| to_err(-32603, format!("serialize: {e}"))),
                        Err(e) => Err(to_err(-32000, format!("tool error: {e}"))),
                    },
                    None => Err(to_err(-32601, format!("unknown tool: {}", params.name))),
                }
            }
            "ping" => Ok(serde_json::json!({})),
            "shutdown" => Ok(serde_json::json!({})),
            other => Err(to_err(-32601, format!("method not found: {other}"))),
        };

        Some(match result {
            Ok(v) => JsonRpcResponse {
                jsonrpc: JSONRPC_VERSION.into(),
                id,
                result: Some(v),
                error: None,
            },
            Err(e) => JsonRpcResponse {
                jsonrpc: JSONRPC_VERSION.into(),
                id,
                result: None,
                error: Some(e),
            },
        })
    }
}

pub fn err_resp(id: serde_json::Value, code: i32, msg: impl Into<String>) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: JSONRPC_VERSION.into(),
        id,
        result: None,
        error: Some(to_err(code, msg)),
    }
}

pub fn to_err(code: i32, msg: impl Into<String>) -> JsonRpcError {
    JsonRpcError { code, message: msg.into(), data: None }
}

pub fn text_content(s: impl Into<String>) -> CallToolResult {
    CallToolResult {
        content: vec![ContentItem::Text { text: s.into() }],
        is_error: false,
    }
}
