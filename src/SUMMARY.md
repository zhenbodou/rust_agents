# Summary

[前言 · 如何阅读本书](./preface.md)

---

# 第一部分 · 基础篇

- [第 1 章 什么是 AI Agent](./part1/ch01-what-is-agent.md)
- [第 2 章 LLM 工作原理与 Agent 的关系](./part1/ch02-llm-basics.md)
- [第 3 章 Rust 环境与工程脚手架](./part1/ch03-rust-setup.md)

# 第二部分 · Agent 核心构建

- [第 4 章 调用 LLM API (Anthropic / OpenAI)](./part2/ch04-llm-api.md)
- [第 5 章 Prompt 工程与结构化输出](./part2/ch05-prompt-engineering.md)
- [第 6 章 Tool Use：让模型调用函数](./part2/ch06-tool-use.md)
- [第 7 章 Agent Loop：ReAct 与 Tool-calling 循环](./part2/ch07-agent-loop.md)
- [第 8 章 记忆系统：短期 / 长期 / 向量检索](./part2/ch08-memory.md)

# 第三部分 · Harness Engineering

- [第 9 章 什么是 Harness Engineer](./part3/ch09-harness-intro.md)
- [第 10 章 Context Engineering：上下文即产品](./part3/ch10-context-engineering.md)
- [第 11 章 权限系统与沙箱](./part3/ch11-permissions.md)
- [第 12 章 Hooks：事件驱动的可扩展点](./part3/ch12-hooks.md)
- [第 13 章 Skills、Slash Commands 与 Workflows](./part3/ch13-skills.md)
- [第 14 章 Subagents 与任务分解](./part3/ch14-subagents.md)
- [第 14 章 补充 A · MCP 协议深入](./part3/ch14a-mcp.md)
- [第 14 章 补充 B · Skills 进阶：编写、分发、版本化](./part3/ch14b-skills-advanced.md)

# 第四部分 · 生产级 / 企业级工程

- [第 15 章 可观测性：日志、Trace、Metrics](./part4/ch15-observability.md)
- [第 16 章 Prompt Caching 与成本优化](./part4/ch16-cost.md)
- [第 17 章 错误处理、重试、限流与熔断](./part4/ch17-reliability.md)
- [第 18 章 Evals：Agent 的测试与评估体系](./part4/ch18-evals.md)
- [第 19 章 安全：Prompt Injection 与数据泄露防御](./part4/ch19-security.md)

# 第五部分 · 实战 · mini-claude-code (Rust)

- [第 20 章 项目架构总览](./part5/ch20-architecture.md)
- [第 21 章 CLI / TUI 与消息渲染](./part5/ch21-cli.md)
- [第 22 章 工具系统：Read / Write / Edit / Bash / Grep](./part5/ch22-tools.md)
- [第 23 章 Agent 主循环与流式输出](./part5/ch23-main-loop.md)
- [第 24 章 权限与 Hooks 实现](./part5/ch24-perms-hooks.md)
- [第 25 章 Session 与持久化记忆](./part5/ch25-session.md)
- [第 26 章 Subagent 并行执行](./part5/ch26-subagent.md)
- [第 27 章 打包、发布与自托管](./part5/ch27-deploy.md)

# 第六部分 · 求职与进阶

- [第 28 章 简历、项目与作品集](./part6/ch28-resume.md)
- [第 29 章 高频面试题 40 讲](./part6/ch29-interview.md)
- [第 30 章 持续学习路线图](./part6/ch30-roadmap.md)

---

[附录 A · 常见问题 FAQ](./appendix/faq.md)
[附录 B · 术语表](./appendix/glossary.md)
[附录 C · 参考资料](./appendix/references.md)
