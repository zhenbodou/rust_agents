# 第 27 章 打包、发布与自托管

> 到这里 mini-claude-code 已经能跑。本章把它变成**别人能装、你能升级、企业能自托管**的真正产品。

## 27.1 产物目标

三种分发形式：

| 形式 | 受众 | 工具 |
|---|---|---|
| `cargo install` | Rust 开发者 | crates.io |
| 预编译二进制 | 终端用户 | GitHub Releases + cargo-dist |
| Docker 镜像 | 服务器 / CI | Dockerfile |

## 27.2 Cargo.toml 最终化

`crates/mcc-cli/Cargo.toml`：

```toml
[package]
name = "mini-claude-code"
version = "0.1.0"
edition = "2021"
license = "MIT"
description = "A Rust CLI coding assistant with tool use, hooks, and subagents."
readme = "../../README.md"
repository = "https://github.com/you/mini-claude-code"
keywords = ["llm", "agent", "cli", "anthropic", "claude"]
categories = ["command-line-utilities", "development-tools"]

[[bin]]
name = "mcc"
path = "src/main.rs"

[dependencies]
mcc-core     = { workspace = true }
mcc-llm      = { workspace = true }
mcc-tools    = { workspace = true }
mcc-harness  = { workspace = true }
mcc-session  = { workspace = true }
mcc-config   = { workspace = true }
mcc-tui      = { workspace = true }
tokio        = { workspace = true }
clap         = { workspace = true }
dotenvy      = { workspace = true }
anyhow       = { workspace = true }
tracing      = { workspace = true }
tracing-subscriber = { workspace = true }
dirs = "5"
```

## 27.3 静态二进制

`.cargo/config.toml`（跨平台 musl 静态链接）：

```toml
[target.x86_64-unknown-linux-musl]
rustflags = ["-C", "target-feature=+crt-static"]
```

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl -p mini-claude-code
# 产物：target/.../release/mcc，完全静态
```

macOS / Windows 用 [`cargo-dist`](https://github.com/axodotdev/cargo-dist) 一键产出多平台 release：

```bash
cargo install cargo-dist
cargo dist init
cargo dist plan
```

生成 GitHub Actions workflow，tag 一发 → 自动 build + 上传 release。

## 27.4 Dockerfile

```dockerfile
# --- builder ---
FROM rust:1.80-slim AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY . .
RUN cargo build --release -p mini-claude-code

# --- runtime ---
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates bash jq git ripgrep bubblewrap && rm -rf /var/lib/apt/lists/*
RUN useradd -m -s /bin/bash agent
COPY --from=builder /app/target/release/mcc /usr/local/bin/mcc
USER agent
WORKDIR /workspace
ENTRYPOINT ["mcc"]
```

`docker run -e ANTHROPIC_API_KEY -v $PWD:/workspace mini-claude-code -p "list project files"` 即可。

## 27.5 自动更新

给二进制加 `mcc --version` + 启动时比对 GitHub latest release：

```rust
pub async fn check_update(current: &str) -> Option<String> {
    let resp = reqwest::get("https://api.github.com/repos/you/mini-claude-code/releases/latest")
        .await.ok()?.json::<serde_json::Value>().await.ok()?;
    let latest = resp["tag_name"].as_str()?.trim_start_matches('v');
    if latest > current { Some(latest.to_string()) } else { None }
}
```

TUI 启动时 banner 提示"新版本可用"。

## 27.6 企业自托管 LLM

很多客户不能把代码发到外部。`mcc-llm` 提供 `BaseUrlOverride`：

```bash
mcc --llm-base-url https://llm-gateway.internal.corp/v1 ...
```

内网常见：`vllm` / `SGLang` / `Ollama` / 商用 LLM 网关。都支持 OpenAI 兼容协议，切到 `OpenAiClient` 即可。

## 27.7 分发技巧

### 27.7.1 首次运行引导

用户第一次跑 `mcc` 时没 API key，给个引导：

```text
欢迎使用 mini-claude-code！

请先配置 API key：
  export ANTHROPIC_API_KEY=sk-ant-...

或写入 ~/.mcc/settings.json:
  { "llm": { "api_key": "..." } }

查阅文档: https://github.com/you/mini-claude-code
```

### 27.7.2 Shell 自动补全

```bash
mcc completion bash > /etc/bash_completion.d/mcc
```

用 `clap_complete` crate：

```rust
use clap_complete::{generate, shells::*};

if let Some(Cmd::Completion { shell }) = args.cmd {
    let mut cmd = Cli::command();
    let bin = "mcc";
    match shell {
        ShellKind::Bash => generate(Bash, &mut cmd, bin, &mut std::io::stdout()),
        ShellKind::Zsh  => generate(Zsh,  &mut cmd, bin, &mut std::io::stdout()),
        ShellKind::Fish => generate(Fish, &mut cmd, bin, &mut std::io::stdout()),
    }
    return Ok(());
}
```

### 27.7.3 Telemetry（可选，且必须可关）

- 默认关
- 打开后只发：版本、OS、功能使用计数（**绝不发 prompt / 代码**）
- 单独的 `mcc telemetry off` 命令
- 隐私政策写在 README 顶部

## 27.8 CI：发布与保质

```yaml
name: release
on:
  push:
    tags: ["v*"]
jobs:
  build:
    strategy:
      matrix:
        target: [x86_64-unknown-linux-musl, aarch64-apple-darwin, x86_64-pc-windows-msvc]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { targets: ${{ matrix.target }} }
      - run: cargo build --release --target ${{ matrix.target }} -p mini-claude-code
      - uses: softprops/action-gh-release@v2
        with: { files: target/${{ matrix.target }}/release/mcc* }
```

每次发布前 CI 跑 `cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo nextest run`、`mcc eval run --suite evals/regression.yaml`。

## 27.9 产品化 Checklist

- [ ] `cargo install --path crates/mcc-cli` 能装
- [ ] `mcc --help` 文档清晰
- [ ] 首次运行有友好引导
- [ ] 配置合并按预期（home + project）
- [ ] 主要 tool 覆盖 eval
- [ ] 权限 deny 规则攻防测试过
- [ ] Session 能 resume，list 能看
- [ ] `/analyze-repo` 子 Agent 并行跑通
- [ ] 日志 JSON 格式 + 脱敏
- [ ] GitHub Actions 自动 release
- [ ] Docker 镜像 < 100MB

## 27.10 小结

- 静态二进制 + cargo-dist 负责分发
- Docker + 自托管 LLM 让企业能用
- 自动更新 + shell 补全是 "产品化" 的细节分
- Checklist 走完，你就有一个可发布的作品集项目

> 🎉 **Part 5 完结**！你已经亲手造了一个 Claude Code 式编码助手的 Rust 版本。下一部分教你把这个作品变成 offer。

