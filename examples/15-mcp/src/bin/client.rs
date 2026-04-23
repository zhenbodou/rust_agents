//! Demo MCP client: spawn 同 crate 的 server 二进制并调用其工具。
//!
//! 运行：
//! ```bash
//! cargo build -p ex15-mcp
//! cargo run -p ex15-mcp --bin mcp-client-demo
//! ```
//!
//! 客户端会 spawn `target/debug/mcp-server-demo` 并通过 stdio 交互。

use anyhow::{Context, Result};
use ex15_mcp::client::McpClient;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_writer(std::io::stderr)
        .init();

    // 找到同 workspace 内 server 二进制。
    let exe = std::env::current_exe()?;
    let dir = exe.parent().context("exe parent")?;
    let server_path = dir.join(if cfg!(windows) {
        "mcp-server-demo.exe"
    } else {
        "mcp-server-demo"
    });

    if !server_path.exists() {
        eprintln!(
            "server binary not found at {}. Run `cargo build -p ex15-mcp` first.",
            server_path.display()
        );
        return Ok(());
    }

    let client = McpClient::connect_stdio(server_path.to_str().unwrap(), &[]).await?;

    let tools = client.list_tools().await?;
    println!("Server exposes {} tools:", tools.len());
    for t in &tools {
        println!("  - {}: {}", t.name, t.description);
    }

    let r = client
        .call_tool("echo", serde_json::json!({"text": "hello MCP"}))
        .await?;
    println!("\necho => {:?}", r);

    let r = client
        .call_tool("add", serde_json::json!({"a": 40, "b": 2}))
        .await?;
    println!("add(40,2) => {:?}", r);

    Ok(())
}
