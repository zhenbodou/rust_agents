# 前言 · 如何阅读本书

## 这本书是为谁写的

你是否属于下面任何一种：

- **编程初学者**，听说 AI Agent 很火，想入门但不知道从哪开始；
- **普通后端 / 前端工程师**，想转型到 AI 应用工程，但被"AI 算法"吓退；
- **有 Rust 背景**的系统工程师，想进入 AI 基础设施 / Harness Engineer 方向；
- **求职者**，想在 1–3 个月内拿下 AI Agent 开发的高级岗位。

如果是，这本书就是为你写的。

## 本书的核心承诺

1. **从零开始**：不假设你懂机器学习，也不假设你是 Rust 高手。每个概念第一次出现都会解释。
2. **全部 Rust 实现**：所有示例代码均为 Rust，可直接 `cargo build` 运行。我们不用 Python。
3. **生产级**：代码不是"Hello World"玩具，而是带错误处理、日志、配置、测试、可观测性的企业级范式。
4. **完整实战**：第五部分会带你用 Rust 从零实现一个 **mini-claude-code**——一个能读文件、写代码、调用 shell、带权限系统、支持子 Agent 的编码助手。

## 什么是 "Harness Engineer"

> 术语来自 Anthropic：Claude Code 的"harness"指包裹 LLM 的那一层——权限、工具、上下文、Hooks、Subagent、Session……
> Harness Engineer 就是**设计和建造这层脚手架**的人。

简单说：算法工程师训练模型，**Harness Engineer 让模型在真实世界里好用**。这是目前市场上最稀缺、薪资最高的 AI 工程方向之一。

## 读法建议

| 你的身份 | 建议路径 |
|---|---|
| 完全小白 | 按顺序读完 Part 1–2，再跳到 Part 5 跟做项目，回头看 Part 3–4 |
| 有后端经验 | Part 1 快速扫过，Part 2–5 细读 |
| 只想看 Claude Code 怎么做的 | 先读 Part 3（概念），再直接看 Part 5（代码） |
| 正在求职 | 读完后务必做完 Part 5，再啃 Part 6 的 40 道面试题 |

## 配套代码仓库

本书所有代码放在仓库的 `examples/` 和 `mini-claude-code/` 目录中：

```
agents_tutorial/
├── book.toml
├── src/                      # mdbook 章节源码
├── examples/                 # 每章独立示例 (cargo workspace member)
│   ├── 04-llm-api/
│   ├── 06-tool-use/
│   ├── 07-agent-loop/
│   └── ...
└── mini-claude-code/         # Part 5 完整项目
    ├── Cargo.toml
    ├── src/
    └── tests/
```

## 需要的准备

- Rust 1.75+（推荐 stable 最新）
- 一个 Anthropic API Key（`ANTHROPIC_API_KEY`）或 OpenAI API Key
- 基本命令行使用能力

准备好了？翻到下一页。

