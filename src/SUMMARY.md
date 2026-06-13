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

# 第七部分 · 前端工程：从零到精通 (HTML / JS / TS / React)

- [第 31 章 Web 与前端基础：浏览器、HTML、CSS 从零开始](./part7/ch31-web-basics.md)
- [第 32 章 JavaScript 从零开始](./part7/ch32-javascript.md)
- [第 33 章 JavaScript 进阶：闭包、原型、异步与事件循环](./part7/ch33-javascript-advanced.md)
- [第 34 章 TypeScript：从零到工程级](./part7/ch34-typescript.md)
- [第 35 章 React 入门：从第一个组件到完整应用](./part7/ch35-react-basics.md)
- [第 36 章 React 进阶与现代前端工程化](./part7/ch36-react-advanced.md)
- [第 37 章 流式 UI：SSE / WebSocket 渲染 Agent 输出](./part7/ch37-streaming-ui.md)
- [第 38 章 轨迹查看器：Trace Viewer 与可视化](./part7/ch38-trace-viewer.md)
- [第 38 章 补充 · 前端精通：测试、可访问性与性能工程](./part7/ch38a-frontend-mastery.md)

# 第八部分 · 后端与 Python：从零到精通

- [第 39 章 后端基础：HTTP、REST API 与数据库从零开始](./part8/ch39-backend-basics.md)
- [第 40 章 Python 从零开始](./part8/ch40-python-basics.md)
- [第 41 章 Python 进阶：类型、asyncio 与工程化](./part8/ch41-python-advanced.md)
- [第 42 章 Agent Scaffold 对接：LangChain / LangGraph / OpenAI Agents SDK](./part8/ch42-scaffolds.md)
- [第 43 章 Agent × RL：从 RL 基础到 Rollout 数据管线](./part8/ch43-agent-rl.md)
- [第 43 章 补充 · Python 精通：pytest、打包发布与性能](./part8/ch43a-python-mastery.md)

# 第九部分 · 容器化与 DevOps：从零到精通

- [第 44 章 Linux 与命令行：从零开始](./part9/ch44-linux-basics.md)
- [第 45 章 Docker：从第一个容器到生产镜像](./part9/ch45-docker.md)
- [第 46 章 Kubernetes：从 kubectl 入门到 Agent 服务运维](./part9/ch46-kubernetes.md)
- [第 47 章 Agent 容器服务：沙箱环境、镜像与依赖管理](./part9/ch47-agent-containers.md)
- [第 48 章 Git 工作流、CI/CD 与代码审查](./part9/ch48-git-cicd.md)
- [第 48 章 补充 · DevOps 精通：供应链安全、密钥治理与 SRE](./part9/ch48a-devops-mastery.md)

# 第十部分 · 实战 · agent-eval-platform

- [第 49 章 评测平台架构总览](./part10/ch49-platform-architecture.md)
- [第 50 章 后端实现：运行编排、轨迹存储与 API](./part10/ch50-backend.md)
- [第 51 章 前端实现：轨迹回放、对比与仪表盘](./part10/ch51-frontend.md)
- [第 52 章 部署与生产运维：Compose → K8s → CI/CD](./part10/ch52-production.md)

---

[附录 A · 常见问题 FAQ](./appendix/faq.md)
[附录 B · 术语表](./appendix/glossary.md)
[附录 C · 参考资料](./appendix/references.md)
[附录 D · 真实生产级开源项目学习路线](./appendix/oss-projects.md)
