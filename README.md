# AI Agent 开发与 Harness Engineering 实战 (Rust 版)

一本面向**中文读者**、**以 Rust 为载体**、**以工业级实战为目标**的 AI Agent 开发教程。

## 本仓库包含什么

- `src/`：mdbook 源码（30 章 + 附录）
- `examples/`：每章独立的 Rust 示例工程
- `mini-claude-code/`：第五部分完整的 Claude Code 风格编码助手 Rust 实现

## 快速开始

```bash
# 安装 mdbook
cargo install mdbook

# 本地预览
mdbook serve --open

# 构建静态站点
mdbook build

# 运行某章示例
cd examples/04-llm-api && cargo run
```

## 环境变量

```bash
export ANTHROPIC_API_KEY=sk-ant-...
# 或
export OPENAI_API_KEY=sk-...
```

## 许可证

MIT
