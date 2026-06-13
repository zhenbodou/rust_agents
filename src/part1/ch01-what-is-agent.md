# 第 1 章 什么是 AI Agent

## 1.1 一个生活中的类比

想象你雇了一位**新入职的助理**。你只告诉他目标："帮我整理一下 Q1 的销售数据，做个周报发给老板。" 他会：

1. **理解**目标；
2. **拆解**任务：先去数据库查数据 → 写脚本汇总 → 画图 → 写邮件 → 发送；
3. **调用工具**：数据库客户端、Python、Excel、邮箱；
4. **遇到问题会查资料、问人**，必要时调整计划；
5. 完成后把结果交给你。

这个助理就是一个 **Agent**。

**AI Agent 就是把上述"助理"换成一个大语言模型（LLM）**，让它具备：

- 理解自然语言目标的能力（来自 LLM 本身）
- 调用工具（函数、API、shell）的能力
- 循环决策（看到工具结果后决定下一步）的能力
- 记忆（跨回合保留信息）的能力

## 1.2 Agent vs Chatbot vs Workflow

很多人把这三者混为一谈。区别是：

| 类型 | 决策者 | 流程 | 工具 | 举例 |
|---|---|---|---|---|
| **Chatbot** | 人 | 一问一答 | 无或极少 | ChatGPT 早期版本、客服机器人 |
| **Workflow** | 开发者（写死） | 预定义步骤 | 固定 | n8n、Zapier、LangChain Chain |
| **Agent** | LLM 自己 | 动态循环 | 按需调用 | Claude Code、Cursor Agent、Devin |

**关键判据**：控制流是**硬编码**的还是 **LLM 动态决定**的？

> 引用 Anthropic 的定义：
> *"Workflows are systems where LLMs and tools are orchestrated through predefined code paths. Agents, on the other hand, are systems where LLMs dynamically direct their own processes and tool usage."*

一个更细致的区分标准：

- **Chatbot** = 无记忆、无工具、单轮问答
- **Workflow** = 有工具、但流程由程序决定，LLM 只是其中一个处理节点
- **Agent** = LLM 主导流程，自己决定调哪个工具、调多少次、什么时候停

## 1.3 Agent 的最小闭环

一个能跑起来的 Agent 至少需要这 4 件事：

```text
        ┌──────────────────────────────────────┐
        │                                      │
        ▼                                      │
   ┌─────────┐     ┌─────────┐     ┌────────────┴─────┐
   │  用户   │ ──► │   LLM   │ ──► │ 决策：调用工具?  │
   │  目标   │     │(大脑)   │     │ 还是回答用户?    │
   └─────────┘     └─────────┘     └────┬─────────────┘
                                        │ 调用工具
                                        ▼
                                  ┌──────────┐
                                  │  工具    │
                                  │(文件/网络│
                                  │ /shell)  │
                                  └──────────┘
```

这个循环叫 **Agent Loop**。看起来简单，真正做好它需要 20+ 章内容。

## 1.4 一个最小的 Rust Agent（伪代码先行）

我们先用伪代码把 Agent Loop 写出来，后面章节会把每个空格填满：

```rust
// 后续第 7 章会给出完整可运行版本
async fn run_agent(user_goal: &str, tools: &[Tool]) -> Result<String> {
    let mut messages = vec![Message::user(user_goal)];
    loop {
        let response = llm.chat(&messages, tools).await?;
        messages.push(response.clone().into());

        match response.stop_reason {
            StopReason::EndTurn => return Ok(response.text()),
            StopReason::ToolUse => {
                for call in response.tool_calls() {
                    let result = execute_tool(&call, tools).await?;
                    messages.push(Message::tool_result(call.id, result));
                }
            }
        }
    }
}
```

这 15 行就是所有 Agent 的灵魂。Claude Code、Cursor Agent、Devin，本质都在跑这个循环。区别在于：

- `tools` 有多丰富
- `llm` 之外的 **harness**（权限、Hooks、Subagent、记忆…）做得有多好
- **上下文工程**做得多精细

本书主要讲的就是这些"区别"。

## 1.5 Agent 的典型应用形态（2025/2026 全景）

2024 年是 Agent 元年，2025 年进入爆发期。按应用场景分类：

| 形态 | 代表产品 | 核心工具 | 成熟度 |
|---|---|---|---|
| **编码助手** | Claude Code, Cursor, Aider, Goose | 文件读写、shell、grep | ★★★★★ 最成熟 |
| **浏览器 Agent** | Browser Use, Anthropic Computer Use | 点击、输入、截图 | ★★★★☆ 成熟 |
| **研究助手** | Perplexity, OpenAI Deep Research | 搜索、阅读网页 | ★★★★☆ 成熟 |
| **数据 Agent** | Julius, Hex Magic | SQL、Python 执行 | ★★★★☆ 成熟 |
| **运维 Agent** | Cleric, incident.io AI | 日志查询、监控、Runbook | ★★★☆☆ 增长中 |
| **邮件/日历 Agent** | Google Workspace AI, Notion AI | 邮件、日历、文档 | ★★★☆☆ 增长中 |
| **客服 Agent** | Sierra, Intercom Fin | 对话、查单、操作后台 | ★★★★☆ 成熟 |
| **科学 Agent** | AlphaFold 3, 各药企 AI | 分子模拟、文献检索 | ★★★☆☆ 专业领域 |

**本书的实战项目 mini-claude-code** 属于第一类——最成熟、最赚钱、最适合拿来当求职作品集。

## 1.6 Agent 的三种架构模式

Agent 不只有"单个 LLM 跑循环"这一种形态，随着任务复杂度提升，出现了三种主流架构：

### 模式一：单 Agent（Single Agent）

```
用户 → [LLM + 工具集] → 结果
```

适合：任务边界清晰、工具集不超过 20 个、不需要并行执行。这是本书 Part 2 重点。

### 模式二：多 Agent（Multi-Agent / Subagent）

```
用户 → 主 Agent（Orchestrator）
            ├─► 子 Agent A（写代码）
            ├─► 子 Agent B（查文档）
            └─► 子 Agent C（跑测试）
```

适合：任务可以分解、需要并行执行、不同子任务需要不同工具集或系统提示。本书 Part 3/5 重点。

### 模式三：流水线（Pipeline Agent）

```
用户 → Agent 1（理解需求）→ Agent 2（生成代码）→ Agent 3（审查）→ 结果
```

适合：任务有固定的阶段性流程、每阶段有验证点、需要人工审批节点。LangGraph 的强项。

> **选择原则**：先用单 Agent，不够用了再加子 Agent，复杂流程才用流水线。过度设计是 Agent 系统失败的主要原因之一。

## 1.7 Agent 为什么"现在"变得可行

在 2022 年前，AI Agent 是学术概念，为什么 2023 年后突然实用了？三个关键变化：

**变化一：模型的工具调用能力质的飞跃**

GPT-4（2023 年 3 月）首次提供稳定的 Function Calling 接口，模型能可靠地输出结构化 JSON 来表达工具调用意图，而不是在文本里乱塞命令。此前的模型经常输出格式错误，Agent 系统极不稳定。

**变化二：上下文窗口大幅扩展**

GPT-3（2020）：4K tokens。GPT-4 Turbo（2023）：128K tokens。Claude 3（2024）：200K tokens。Claude 3.5/4（2025）：1M tokens。

工具调用的历史记录和观察结果会快速消耗 context，4K tokens 根本跑不了复杂任务。128K+ 才让真实的多步 Agent 成为可能。

**变化三：模型的指令遵循能力大幅提升**

RLHF/RLAIF 训练让模型能更忠实地遵循复杂的系统提示，包括"只在需要时调用工具"、"遇到不确定时询问而不是猜测"等对 Agent 至关重要的行为规范。早期模型时常"罔顾指令"，现代模型则能高度遵守约束。

## 1.8 Agent 的常见失败模式

Agent 不是万能的，理解它的局限是构建可靠系统的前提：

| 失败模式 | 描述 | 典型表现 | 对策 |
|---|---|---|---|
| **幻觉传播** | 错误的工具调用结果进入上下文，污染后续决策 | 读了不存在的文件后继续"修改"它 | 工具结果校验 + Grounding |
| **死循环** | Agent 反复做同一件无效操作 | 无限次 `grep` 找不到就再 `grep` | 迭代次数上限 + 循环检测 |
| **过度自信** | 模型凭借训练记忆回答，不用工具验证 | 声称文件内容是某样，其实没读 | 强制要求"先读后写" |
| **工具滥用** | 用复杂工具解决简单问题 | 调 shell 脚本做一个字符串替换 | 工具设计时明确使用场景 |
| **上下文爆炸** | 工具输出过大塞满 context | 一次 `cat` 了一个 10MB 的日志 | 工具输出截断 + 分页 |
| **权限蔓延** | Agent 做了超出预期范围的操作 | "顺便"删掉了以为没用的目录 | 最小权限原则 + 沙箱 |
| **Reward Hacking** | 找到满足评分标准的捷径但不解决真实问题 | 删掉失败的测试来让测试通过 | 测试文件只读 + 独立评分环境 |

这些失败模式会贯穿全书逐一讲解如何应对。

## 1.9 一个完整 Agent 所需的工程层次

把 Agent 想成一栋楼，从底层到顶层：

```
┌──────────────────────────────────────┐  ← 应用层：用户界面、产品逻辑
├──────────────────────────────────────┤
│  评测层：Evals / Benchmarks           │  ← Part 4 重点
├──────────────────────────────────────┤
│  Harness 层：权限、Hooks、Skills、      │
│             Subagents、Context Eng.  │  ← Part 3 重点
├──────────────────────────────────────┤
│  Agent Loop 层：循环、工具、记忆        │  ← Part 2 重点
├──────────────────────────────────────┤
│  LLM API 层：调用、流式、缓存           │  ← Part 2 重点
├──────────────────────────────────────┤
│  基础设施层：容器、K8s、CI/CD           │  ← Part 9 重点
└──────────────────────────────────────┘  ← 底层：Linux、网络、存储
```

**本书的覆盖范围：从底到顶全部覆盖**。你不只学会"接个 API 调 LLM"，而是能设计和构建整栋楼。

## 1.10 小结

- Agent = LLM + 工具 + 循环决策；与 Workflow 的本质区别是控制流由谁决定
- 三种架构模式：单 Agent、多 Agent（Subagent）、流水线
- Agent"现在"可行的三大原因：工具调用成熟、上下文窗口扩大、指令遵循能力提升
- 七种常见失败模式：幻觉传播、死循环、过度自信、工具滥用、上下文爆炸、权限蔓延、Reward Hacking
- 最小闭环是 **Agent Loop**，后面所有内容都是在丰富和强化这个循环

> **下一章**：补上"LLM 到底是怎么工作的"这一课——从 Transformer 的核心直觉，到训练管线，到实际调参建议。
