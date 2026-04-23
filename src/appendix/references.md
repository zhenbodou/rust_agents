# 附录 C · 参考资料

## 官方文档

- [Anthropic API Documentation](https://docs.anthropic.com)
- [Anthropic Cookbook (GitHub)](https://github.com/anthropics/anthropic-cookbook)
- [Claude Code Documentation](https://docs.anthropic.com/claude-code)
- [OpenAI API Documentation](https://platform.openai.com/docs)
- [OpenAI Cookbook](https://cookbook.openai.com)
- [Model Context Protocol](https://modelcontextprotocol.io)

## 必读文章

- Anthropic — [Building Effective Agents](https://www.anthropic.com/research/building-effective-agents)
- Lilian Weng — [LLM Powered Autonomous Agents](https://lilianweng.github.io/posts/2023-06-23-agent/)
- Anthropic — [Context Engineering for Coding Agents](https://www.anthropic.com/news)（不定期更新）

## 经典论文

- ReAct: Synergizing Reasoning and Acting in Language Models
- Toolformer: Language Models Can Teach Themselves to Use Tools
- Reflexion: Language Agents with Verbal Reinforcement Learning
- Self-Consistency Improves Chain of Thought Reasoning
- Constitutional AI: Harmlessness from AI Feedback
- MemGPT: Towards LLMs as Operating Systems
- Lost in the Middle: How Language Models Use Long Contexts

## 开源项目

| 项目 | 语言 | 链接 |
|---|---|---|
| Claude Code | TS/Node | https://github.com/anthropics/claude-code |
| Aider | Python | https://github.com/Aider-AI/aider |
| OpenHands | Python | https://github.com/All-Hands-AI/OpenHands |
| Continue | TS | https://github.com/continuedev/continue |
| Goose | Rust | https://github.com/block/goose |
| rig | Rust | https://github.com/0xPlaygrounds/rig |
| swiftide | Rust | https://github.com/bosun-ai/swiftide |
| MCP Servers | 多 | https://github.com/modelcontextprotocol/servers |

## Rust 生态

- [tokio](https://tokio.rs) — 异步运行时
- [reqwest](https://docs.rs/reqwest) — HTTP 客户端
- [ratatui](https://ratatui.rs) — TUI 框架
- [tracing](https://tracing.rs) — 结构化日志
- [clap](https://docs.rs/clap) — CLI 解析
- [serde](https://serde.rs) — 序列化
- [ignore](https://docs.rs/ignore) — ripgrep 核心
- [grep](https://docs.rs/grep) — ripgrep 搜索引擎
- [governor](https://docs.rs/governor) — 限流
- [globset](https://docs.rs/globset) — Glob 匹配

## Benchmarks / Evals

- [SWE-bench](https://www.swebench.com) — 修 GitHub issue
- [TAU-bench](https://github.com/sierra-research/tau-bench) — 工具使用
- [GAIA](https://huggingface.co/gaia-benchmark) — 通用 Agent 任务
- [AgentBench](https://github.com/THUDM/AgentBench)

## 观测平台

- Honeycomb / Datadog / New Relic — 通用
- [Langfuse](https://langfuse.com) — LLM 专用，可自托管
- [Helicone](https://helicone.ai) — LLM 可观测 SaaS
- [LangSmith](https://smith.langchain.com) — LangChain 生态

## 推荐博客 / Twitter

- Simon Willison — https://simonwillison.net
- Anthropic Engineering Blog
- @karpathy — 深度教学
- @swyx — AI 产品思考
- @jxmnop — 有趣的研究

## 中文资源

- 微信公众号：Anthropic 中文社群、AI 前线、机器之心 AI Agent 专题
- 掘金 AI 专题
- 知乎：@张俊林、@李国瑞 等资深 AI 人
- B 站：李宏毅、何恺明公开课

## 本书致谢

本书大量设计参考了 Claude Code、Aider、Cursor、OpenHands 等开源 / 闭源产品的公开资料。感谢这些项目把 AI Agent 推到了今天的成熟度。

任何错误或改进建议，欢迎在本书仓库提 issue。

