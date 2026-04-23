# 第 3 章 Rust 环境与工程脚手架

> 目标：半小时内让你在任意机器上跑起本书的示例，并建立一套**企业级**的 Cargo workspace 目录。

## 3.1 为什么用 Rust 写 Agent

AI 工程主流是 Python，但有三类人应该用 Rust：

1. **基础设施 / Harness 工程师**：Agent runtime 要长跑、低延迟、并发吞吐高，Rust 是最佳选择。
2. **桌面 / CLI 产品开发者**：想做 Claude Code / Cursor 这类本地工具，Rust 静态二进制、启动快、无 GC 抖动。
3. **安全敏感场景**：沙箱、权限系统、不可信 tool 执行——Rust 的内存安全 + 细粒度并发模型天然合适。

Anthropic 自己的 Claude Code 就是以 Node/TS 为主 + 关键路径 Rust（如 ripgrep 嵌入）。Cursor 的核心部分也有 Rust 参与。

## 3.2 安装

```bash
# 1. 安装 rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. 升级到最新 stable
rustup update stable
rustup default stable

# 3. 必备组件
rustup component add clippy rustfmt rust-analyzer

# 4. 常用 cargo 插件
cargo install cargo-watch cargo-expand cargo-nextest mdbook
```

校验：

```bash
rustc --version    # 应 >= 1.75
cargo --version
mdbook --version
```

## 3.3 全书 Cargo Workspace 结构

我们用一个 **workspace** 组织所有代码。根目录 `Cargo.toml`：

```toml
[workspace]
resolver = "2"
members = [
    "examples/04-llm-api",
    "examples/05-structured-output",
    "examples/06-tool-use",
    "examples/07-agent-loop",
    "examples/08-memory",
    "examples/11-permissions",
    "examples/12-hooks",
    "examples/14-subagent",
    "examples/15-observability",
    "examples/16-caching",
    "examples/17-reliability",
    "examples/18-evals",
    "mini-claude-code",
]

[workspace.package]
edition = "2021"
version = "0.1.0"
authors = ["agents-tutorial"]
license = "MIT"
rust-version = "1.75"

[workspace.dependencies]
# 异步运行时
tokio = { version = "1.40", features = ["full"] }
futures = "0.3"

# HTTP / LLM
reqwest = { version = "0.12", features = ["json", "stream", "rustls-tls"], default-features = false }
eventsource-stream = "0.2"

# 序列化
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# 错误 / 日志
anyhow = "1"
thiserror = "2"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }

# 配置
config = "0.14"
dotenvy = "0.15"

# CLI / TUI
clap = { version = "4", features = ["derive", "env"] }
ratatui = "0.28"
crossterm = "0.28"

# 工具
regex = "1"
walkdir = "2"
ignore = "0.4"                 # ripgrep 的核心库
which = "6"
tempfile = "3"
async-trait = "0.1"
once_cell = "1"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4", "serde"] }

# 测试
tokio-test = "0.4"
mockito = "1"
insta = "1"
```

## 3.4 统一的错误与日志基础 crate

我们会有一个共享 crate `agent-core`，放在 `mini-claude-code/crates/agent-core`（从第 20 章详细介绍），现在先了解**两件每个示例都要做的事**：

### 3.4.1 错误类型约定

```rust
// 项目内部错误用 thiserror 定义具体类型
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("LLM API error: {0}")]
    Api(String),

    #[error("Tool `{name}` failed: {source}")]
    Tool { name: String, #[source] source: anyhow::Error },

    #[error("Permission denied: {0}")]
    Permission(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, AgentError>;
```

### 3.4.2 日志初始化（一次到位的生产级配置）

```rust
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,agent=debug"));

    let fmt_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_line_number(true)
        .compact();

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .init();
}
```

使用：

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();
    tracing::info!("agent starting");
    Ok(())
}
```

通过 `RUST_LOG=agent=trace,reqwest=info` 环境变量控制粒度。

## 3.5 管理 API Key

**绝对不要**把 key 硬编到代码里。企业级做法：

1. 本地开发：`.env` 文件 + `dotenvy` 加载
2. CI / 生产：环境变量或 Secrets Manager（AWS SM、Vault）

`.env` 示例：

```ini
ANTHROPIC_API_KEY=sk-ant-xxx
OPENAI_API_KEY=sk-xxx
RUST_LOG=info
```

在 `.gitignore` 里**必须**加：

```
.env
.env.*
!.env.example
```

## 3.6 推荐的 IDE / 编辑器配置

- VSCode + `rust-analyzer` + `Even Better TOML` + `CodeLLDB`
- 或 Cursor / Claude Code 本身（开发 AI 工具用 AI 工具，元得很）
- JetBrains RustRover

## 3.7 验证脚手架

建一个最小 crate 跑通：

```bash
mkdir -p examples/03-hello && cd examples/03-hello
cargo init --name hello-agent
```

把 `Cargo.toml` 改为引用 workspace 依赖（这是 workspace 的关键优势——版本统一）：

```toml
[package]
name = "hello-agent"
edition.workspace = true
version.workspace = true

[dependencies]
tokio.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
anyhow.workspace = true
```

`src/main.rs`：

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    tracing::info!("Hello, Agent!");
    Ok(())
}
```

在 **workspace 根目录**执行：

```bash
cargo run -p hello-agent
# 应输出: INFO hello_agent: Hello, Agent!
```

跑通这一步，意味着后续所有章节的代码你都能直接运行。

## 3.8 小结

- 统一 workspace + 统一依赖版本 = 企业级组织基础
- 日志、错误、配置、secrets——这些"无聊"的东西是招聘官真正看的细节
- 记住：**生产级 Agent 项目 90% 的代码和 LLM 无关**，是工程

> **下一章**：发起第一次 LLM API 调用，写出本书第一个 AI 程序。

