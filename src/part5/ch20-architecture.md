# 第 20 章 mini-claude-code 项目架构总览

> 从现在开始，我们把前 19 章的所有零件，拼成一个真正可用的**类 Claude Code** 编码助手。

## 20.1 产品目标

一个用 Rust 写的 CLI + TUI 工具，支持：

- ✅ 对话式编程助手（读/写/改文件、跑测试）
- ✅ 权限系统（default / acceptEdits / bypass）
- ✅ Hooks（与 Claude Code 格式兼容）
- ✅ Slash commands / Skills
- ✅ Subagents 并行
- ✅ Session 持久化 + 回放
- ✅ Prompt caching
- ✅ 可观测性（JSON 日志 + OTel）
- ✅ 多模型（Anthropic 主，OpenAI 兼容 fallback）

最终二进制命名 `mcc`（mini-claude-code）。

## 20.2 Workspace 布局

```
mini-claude-code/
├── Cargo.toml                     # workspace 根
├── crates/
│   ├── mcc-core/                  # 类型、错误、traits
│   ├── mcc-llm/                   # LLM provider 抽象与实现
│   ├── mcc-tools/                 # 内建工具集合
│   ├── mcc-harness/               # agent loop、permissions、hooks、skills、subagent
│   ├── mcc-session/               # session 持久化 + 记忆
│   ├── mcc-config/                # 配置加载 (settings.json)
│   ├── mcc-tui/                   # Ratatui 前端
│   └── mcc-cli/                   # 最终二进制
├── skills/                        # 示例 skills
├── commands/                      # 示例 slash commands
├── hooks/                         # 示例 hooks
├── tests/                         # 集成测试
└── evals/                         # eval 集
```

## 20.3 模块依赖图

```text
          mcc-cli
             │
        ┌────┴────┐
        │         │
     mcc-tui   mcc-harness
                   │
      ┌────────────┼────────────┐
      │            │            │
   mcc-tools   mcc-session   mcc-llm
      │            │            │
      └────────────┴────┬───────┘
                        │
                    mcc-core
                        │
                    mcc-config
```

**mcc-core** 是零依赖基础层（除了 `serde` / `anyhow`），其他 crate 都依赖它。

## 20.4 顶层 Cargo.toml

```toml
[workspace]
resolver = "2"
members = ["crates/*"]

[workspace.package]
edition = "2021"
version = "0.1.0"
license = "MIT"
rust-version = "1.75"

[workspace.dependencies]
# 基础
tokio = { version = "1.40", features = ["full"] }
futures = "0.3"
async-trait = "0.1"
anyhow = "1"
thiserror = "2"
once_cell = "1"

# 序列化与 IO
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
toml = "0.8"

# HTTP
reqwest = { version = "0.12", features = ["json","stream","rustls-tls"], default-features = false }
eventsource-stream = "0.2"

# 工具
regex = "1"
globset = "0.4"
ignore = "0.4"
walkdir = "2"
which = "6"
tempfile = "3"
uuid = { version = "1", features = ["v4","serde"] }
chrono = { version = "0.4", features = ["serde"] }

# CLI / TUI
clap = { version = "4", features = ["derive","env"] }
ratatui = "0.28"
crossterm = "0.28"

# Observability
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter","json"] }

# 配置
dotenvy = "0.15"

# 内部 crate（指定路径）
mcc-core    = { path = "crates/mcc-core" }
mcc-llm     = { path = "crates/mcc-llm" }
mcc-tools   = { path = "crates/mcc-tools" }
mcc-harness = { path = "crates/mcc-harness" }
mcc-session = { path = "crates/mcc-session" }
mcc-config  = { path = "crates/mcc-config" }
mcc-tui     = { path = "crates/mcc-tui" }

[profile.release]
lto = "thin"
codegen-units = 1
strip = "symbols"
```

## 20.5 配置文件形状

`~/.mcc/settings.json` 与项目级 `.mcc/settings.json` 合并：

```json
{
  "model": {
    "main": "claude-opus-4-7",
    "subagent": "claude-sonnet-4-6",
    "summarize": "claude-haiku-4-5-20251001"
  },
  "permissions": {
    "mode": "default",
    "allow": ["Bash(cargo test:*)","Bash(cargo check:*)","Read(**)"],
    "deny":  ["Write(.env)","Read(.env)","Bash(rm -rf *)","Bash(git push --force*)"]
  },
  "hooks": {
    "PreToolUse":  [{ "matcher": "Bash(git *)", "command": "bash .mcc/hooks/audit-git.sh" }],
    "PostToolUse": [{ "matcher": "Write(*.rs)", "command": "rustfmt $FILE" }]
  },
  "observability": {
    "log_format": "json",
    "otel_endpoint": null
  },
  "budget": {
    "max_usd_per_session": 2.0,
    "max_iterations": 40
  }
}
```

## 20.6 核心时序图

```text
 user 输入 ──► mcc-cli ──► mcc-tui 渲染
                 │
                 └► mcc-harness::AgentLoop
                       │
                       ├─ ContextBuilder (system / skills / memory / ide)
                       ├─ HookDispatcher(UserPromptSubmit)
                       │
                       ├─ llm.stream() ◄─── mcc-llm (Anthropic)
                       │       ▲
                       │       │ SSE events → TUI 流式渲染
                       │
                       ├─ 对每个 tool_use:
                       │     ├─ HookDispatcher(PreToolUse)
                       │     ├─ PermissionChecker.check()
                       │     ├─ tool.execute()
                       │     └─ HookDispatcher(PostToolUse)
                       │
                       ├─ SessionRecorder.record_turn()
                       └─ 循环直到 end_turn / budget / user abort
```

## 20.7 开发里程碑（章节对应）

| 章 | 目标 | 可演示成果 |
|---|---|---|
| 20 | 架构 + Cargo 骨架 | `cargo check` 通过 |
| 21 | CLI + TUI | 能交互、能看 LLM 流式输出 |
| 22 | 工具系统 | read/write/edit/bash/grep/list |
| 23 | Agent 主循环 | 完成"让它写个 fib 函数" |
| 24 | 权限 + Hooks | 危险命令询问；auto-format |
| 25 | Session + 记忆 | `mcc resume`；多 session 历史 |
| 26 | Subagent | 大仓库分析命令 `/analyze-repo` |
| 27 | 打包发布 | `cargo install --path crates/mcc-cli` |

## 20.8 本章小结

- 8 个 crate，职责单一，自底向上
- 配置与 Claude Code 高度兼容（不是偶然——用户迁移成本低）
- 下一章开始写代码

> **下一章**：把 CLI 和 TUI 先跑起来，建立"能看到输出"的最小循环。

