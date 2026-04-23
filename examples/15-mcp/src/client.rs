//! MCP client: spawn 一个 server 子进程通过 stdio 对话。

use crate::protocol::*;
use anyhow::{anyhow, Context, Result};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{oneshot, Mutex};
use std::collections::HashMap;
use std::sync::Arc;

pub struct McpClient {
    _child: Child,
    stdin: Mutex<ChildStdin>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
    next_id: AtomicU64,
}

impl McpClient {
    /// Spawn a server binary and perform the `initialize` handshake.
    pub async fn connect_stdio(program: &str, args: &[&str]) -> Result<Self> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .context("spawn mcp server")?;

        let stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?;

        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        spawn_reader(stdout, pending.clone());

        let c = Self {
            _child: child,
            stdin: Mutex::new(stdin),
            pending,
            next_id: AtomicU64::new(1),
        };

        let _: InitializeResult = c.call("initialize", InitializeParams {
            protocol_version: MCP_PROTOCOL_VERSION.into(),
            capabilities: ClientCapabilities::default(),
            client_info: Implementation {
                name: "ex15-mcp-client".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
        }).await?;

        Ok(c)
    }

    pub async fn list_tools(&self) -> Result<Vec<ToolInfo>> {
        let r: ListToolsResult = self.call("tools/list", serde_json::json!({})).await?;
        Ok(r.tools)
    }

    pub async fn call_tool(&self, name: &str, arguments: serde_json::Value) -> Result<CallToolResult> {
        self.call("tools/call", CallToolParams {
            name: name.into(),
            arguments,
        }).await
    }

    async fn call<P, R>(&self, method: &str, params: P) -> Result<R>
    where
        P: serde::Serialize,
        R: serde::de::DeserializeOwned,
    {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let req = JsonRpcRequest {
            jsonrpc: JSONRPC_VERSION.into(),
            id: Some(serde_json::json!(id)),
            method: method.into(),
            params: Some(serde_json::to_value(params)?),
        };

        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);

        let line = serde_json::to_string(&req)? + "\n";
        {
            let mut stdin = self.stdin.lock().await;
            stdin.write_all(line.as_bytes()).await?;
            stdin.flush().await?;
        }

        let resp = rx
            .await
            .map_err(|_| anyhow!("response channel closed"))?;
        if let Some(err) = resp.error {
            return Err(anyhow!("{} ({})", err.message, err.code));
        }
        let v = resp.result.unwrap_or(serde_json::Value::Null);
        Ok(serde_json::from_value(v)?)
    }
}

fn spawn_reader(
    stdout: ChildStdout,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
) {
    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let Ok(resp) = serde_json::from_str::<JsonRpcResponse>(&line) else { continue };
            if let Some(id) = resp.id.as_u64() {
                if let Some(tx) = pending.lock().await.remove(&id) {
                    let _ = tx.send(resp);
                }
            }
        }
    });
}
