# 第 14 章 补充 A · MCP (Model Context Protocol) 深入

> MCP 是 Anthropic 2024 年开源的 **"LLM 客户端 ↔ 外部能力提供者"标准协议**。理解它等于拿到 Agent 生态的"入场券"。

## A.1 为什么会出现 MCP

每个 Agent 宿主（Claude Code、Cursor、Continue、Cline、Goose…）都要接入一堆工具：文件系统、Git、Jira、Slack、GitHub、数据库…… 没有标准之前，每个产品都要**重写一遍对接**。

MCP 的解决办法：

```text
  ┌───────────────┐        ┌───────────────┐
  │   Host (LLM)  │ ◄──► │  MCP Server   │
  │  Claude Code  │        │  github-mcp  │
  └───────────────┘        └───────────────┘
         │ JSON-RPC 2.0           │
         │ (stdio / SSE / HTTP)   │
         └────────────────────────┘
```

**一次对接（写一次 server），所有 MCP host 都能用。** 类似 LSP 对 IDE。

## A.2 MCP 的三类实体

| 实体 | 职责 | 谁写 |
|---|---|---|
| **Host** | LLM 应用，调 server | Claude Code / 你的 mcc |
| **Server** | 暴露 tools / resources / prompts | 社区 / 企业 / 自己 |
| **Client** | Host 内部的连接器（一个 host 连多个 server） | Host 集成 |

## A.3 Transport

三种常见传输：

| Transport | 场景 |
|---|---|
| **stdio** | 最常用：host spawn server 子进程，双向 JSON-RPC |
| **SSE** | 远程服务，HTTP 长连接 |
| **Streamable HTTP** | 新版，HTTP POST + optional SSE，支持会话 |

本书以 stdio 为主（开发最简单）。

## A.4 JSON-RPC 2.0 基础

每个消息是这样的：

```json
{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}
```

响应：

```json
{"jsonrpc":"2.0","id":1,"result":{"tools":[...]}}
```

无 `id` 的是 **notification**（不期待响应）。

## A.5 MCP 核心消息

### A.5.1 initialize（握手）

client → server：

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{
  "protocolVersion":"2025-03-26",
  "capabilities":{},
  "clientInfo":{"name":"mcc","version":"0.1.0"}
}}
```

server 返回自己支持的 capabilities：`tools` / `resources` / `prompts`。

### A.5.2 tools/list

列出 server 暴露的工具。返回 `ToolInfo[]`：

```json
{"tools":[
  {"name":"read_issue","description":"…","inputSchema":{…}}
]}
```

### A.5.3 tools/call

client 调用：

```json
{"method":"tools/call","params":{"name":"read_issue","arguments":{"id":123}}}
```

server 返回：

```json
{"content":[{"type":"text","text":"…"}],"isError":false}
```

### A.5.4 resources/* 与 prompts/*

除了 tools，MCP 还支持：

- **Resources**：只读数据（文件、API 结果），用 URI 寻址
- **Prompts**：预定义的对话模板（类似 Skill）

本章聚焦 tools，另两者机制类似。

## A.6 Rust 实现：`examples/15-mcp`

对应代码在 [examples/15-mcp](../../../examples/15-mcp/)。包括：

- `protocol.rs`：JSON-RPC + MCP 消息类型
- `server.rs`：stdio server，注册 `McpTool` 实现类
- `client.rs`：spawn server 子进程 + async JSON-RPC 客户端
- `src/bin/server.rs`：暴露 `echo` 与 `add` 的示例 server
- `src/bin/client.rs`：连接上面 server 并调用

### A.6.1 运行

```bash
cargo build -p ex15-mcp
cargo run  -p ex15-mcp --bin mcp-client-demo
```

预期输出：

```
Server exposes 2 tools:
  - echo: Return the input text unchanged.
  - add: Add two integers.

echo => CallToolResult { content: [Text { text: "hello MCP" }], is_error: false }
add(40,2) => CallToolResult { content: [Text { text: "42" }], is_error: false }
```

### A.6.2 核心片段

Server 注册工具（来自 [server.rs](../../../examples/15-mcp/src/server.rs)）：

```rust
let mut server = McpServer::new("demo-server", env!("CARGO_PKG_VERSION"));
server.register(EchoTool);
server.register(AddTool);
server.serve_stdio().await
```

Client 使用（来自 [client.rs](../../../examples/15-mcp/src/client.rs)）：

```rust
let client = McpClient::connect_stdio("./mcp-server-demo", &[]).await?;
let tools = client.list_tools().await?;
let r = client.call_tool("add", json!({"a":40,"b":2})).await?;
```

## A.7 把 MCP server 接进 mini-claude-code

Host 侧做两件事：

### 1. 把 MCP tool 包装成我们自己的 `Tool` trait

```rust
pub struct McpToolAdapter {
    client: Arc<McpClient>,
    info: ToolInfo,
}

#[async_trait]
impl mcc_core::Tool for McpToolAdapter {
    fn name(&self) -> &str { &self.info.name }
    fn description(&self) -> &str { &self.info.description }
    fn input_schema(&self) -> Value { self.info.input_schema.clone() }

    async fn execute(&self, input: Value, _: &ToolContext) -> ToolOutput {
        match self.client.call_tool(&self.info.name, input).await {
            Ok(r) => {
                let text = r.content.iter().map(|c| match c {
                    ContentItem::Text { text } => text.as_str(),
                }).collect::<Vec<_>>().join("\n");
                if r.is_error { ToolOutput::err(text) } else { ToolOutput::ok(text) }
            }
            Err(e) => ToolOutput::err(e.to_string()),
        }
    }
}
```

### 2. 配置文件声明要加载的 MCP servers

与 Claude Code 兼容的格式：

```json
{
  "mcpServers": {
    "github": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"],
      "env": {"GITHUB_TOKEN": "${GITHUB_TOKEN}"}
    },
    "postgres": {
      "command": "mcp-postgres",
      "args": ["postgresql://localhost/mydb"]
    }
  }
}
```

加载时遍历，每个 server spawn 一个 client，list_tools 后把每个工具注册到 registry。

## A.8 生产中的 MCP 注意事项

### A.8.1 安全

MCP server 是**完全信任的代码**——host 会把它的输出塞进 LLM 上下文，工具执行可能有副作用。所以：

- 只跑**已知来源**的 server（类似 brew install 的仓库机制）
- 权限系统对 MCP tool **同样生效**（deny 规则跨 server 适用）
- 长远：签名 + 沙箱隔离（目前生态还在发展）

### A.8.2 性能

- stdio server 每次请求都要进程内 IPC，延迟 ~0.1ms—ok
- 远程 SSE 注意网络延迟和连接复用
- `tools/list` 应该只在启动时调一次，缓存结果

### A.8.3 版本协商

`protocolVersion` 双方要对齐。不同 MCP 版本在消息命名上有差异（比如 `tools/call` vs `invokeTool`）。我们 crate 用的是 2025-03-26 版，目前是主流。

## A.9 生态：值得了解的 MCP server

| 类别 | Server |
|---|---|
| 开发 | github, gitlab, jetbrains-ide |
| 数据 | postgres, sqlite, mongodb, elasticsearch |
| 沟通 | slack, gmail, calendar |
| 监控 | sentry, datadog, prometheus |
| 文档 | notion, confluence |
| 云 | aws, gcp, kubernetes |

官方列表：<https://github.com/modelcontextprotocol/servers>

**Harness Engineer 面试加分项**：你能快速用 Rust 写一个企业内部系统的 MCP server，让团队所有 Agent（Claude Code / Cursor / 自研）**一次接入处处可用**。

## A.10 小结

- MCP = 标准化的 tool/resource/prompt 协议，基于 JSON-RPC 2.0
- stdio 最常用，server = 子进程
- Host 加个 adapter 就能把 MCP tool 当作本地 tool 用
- 权限系统必须覆盖 MCP
- 这是未来 Agent 生态的基础设施，值得深入

> 下一小节：把我们的 Skills 系统升级到"能发布、能订阅、能版本管理"。

