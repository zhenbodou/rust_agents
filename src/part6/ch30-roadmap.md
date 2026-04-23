# 第 30 章 持续学习路线图

> AI Agent 领域每个月都有新东西。这章给你"之后学什么"的地图。

## 30.1 刚入门（0–3 个月）

目标：把本书走完一遍，mini-claude-code 能上 GitHub。

- 完整跑通 Part 2 所有示例
- 实现并发布 mini-claude-code
- 写 3 篇配套博客

## 30.2 进阶（3–12 个月）

### 30.2.1 读官方文档

每周读一次：

- [Anthropic Docs](https://docs.anthropic.com) —— 权威，新 feature 第一手
- [Anthropic Cookbook](https://github.com/anthropics/anthropic-cookbook)
- [OpenAI Cookbook](https://cookbook.openai.com)
- [LangChain / LlamaIndex 博客](https://blog.langchain.dev) —— 哪怕你不用，也能看到生态动态

### 30.2.2 读代码

| 项目 | 为什么值得读 |
|---|---|
| [Claude Code](https://github.com/anthropics/claude-code) | 工业级 Harness 标杆 |
| [Aider](https://github.com/Aider-AI/aider) | Python 版 coding agent，edit 策略值得学 |
| [Goose](https://github.com/block/goose) | Rust 写的 agent，可对比你的实现 |
| [rig](https://github.com/0xPlaygrounds/rig) | Rust LLM framework |
| [OpenHands / SWE-Agent](https://github.com/All-Hands-AI/OpenHands) | swe-bench 冠军级 agent |
| [continue](https://github.com/continuedev/continue) | VSCode 插件式 agent |

**读法**：不要从头读到尾，而是带问题读。"Aider 的 edit 算法怎么处理 diff 冲突？" → 去读对应目录。

### 30.2.3 学理论

- **Toolformer** / **ReAct** / **Reflexion** 等经典论文
- Anthropic 的 ["Building effective agents"](https://www.anthropic.com/research)
- [LLM Powered Autonomous Agents - Lilian Weng](https://lilianweng.github.io/posts/2023-06-23-agent/)

每篇花 1–2 小时精读，写笔记到你的 memory 里。

### 30.2.4 参加比赛

- **swe-bench**：全自动修 GitHub issue 的 benchmark
- Kaggle AI agent 系列
- AgentBench

参赛本身就是强作品集。

## 30.3 专家（1+ 年）

### 30.3.1 专精一个方向

- **Computer Use / GUI Agents**：Anthropic 已开放；方向热但未成熟
- **多 Agent 协作 / 编排**：CrewAI / AutoGen 在做，理论还没定型
- **Agent Security**：红队、可信执行、supply chain
- **Evals Science**：eval 建模本身是前沿
- **MCP (Model Context Protocol)**：Anthropic 开源的标准化协议，将成为生态基础

### 30.3.2 参与标准

- MCP：[Model Context Protocol 规范](https://modelcontextprotocol.io)
- 贡献 server / client 或规范 RFC
- 加入相关 working group

### 30.3.3 写代码 / 写文章 / 讲

1 年内达成：1 个开源项目 ⭐ 500+；10 篇技术文章；3 次 meetup 分享。**影响力本身就是你的生产力工具**。

## 30.4 每周节奏建议

| 日 | 内容 |
|---|---|
| 周一 | 读 1 篇论文或官方 changelog |
| 周三 | 读 500 行开源代码，写读码笔记 |
| 周五 | 给 mini-claude-code 加一个 feature 或 eval |
| 周末 | 写一篇博客 / 录一段演示 |

## 30.5 社区

- Discord：Anthropic / OpenAI 官方、LangChain、MCP 等
- Twitter/X：@AnthropicAI, @OpenAI, @lilianweng, @karpathy, @simonw
- 中文社区：掘金 AI 专题、知乎、微信群（关注 Cursor / Claude 中文群）

## 30.6 心态

**避免三种陷阱**：

1. **框架病**：永远在追新框架，从不深入某一个。答案：选一个自己写的（你的 mini-claude-code）深耕。
2. **论文焦虑**：论文无穷多。答案：只读和你当前作品相关的。
3. **速朽恐惧**：模型/API 一直变。答案：记住 **Harness 是稳态技能**——系统工程、权限、eval、可观测，和哪个模型无关。

## 30.7 离开这本书之前

你现在应该能：

- ✅ 独立设计并实现一个编码 Agent
- ✅ 讲清楚 Agent、Harness、Context、Hook、Subagent 的关系
- ✅ 写出带错误处理、观测、权限、eval 的生产代码
- ✅ 应对大部分 AI Agent 工程师面试
- ✅ 持续学习并跟上领域演化

## 30.8 最后的话

> AI Agent 不是魔法，是好工程。把每一章的"无聊"细节做扎实——日志格式、错误分类、权限规则、eval 数据——你就会比 90% 的 "AI 工程师"更值钱。
>
> 祝你顺利拿到心仪的 offer。之后，记得回来给这本书提 issue 和 PR。

—— The End of the Book（暂时）

