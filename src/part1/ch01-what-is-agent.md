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

| 类型 | 决策者 | 流程 | 举例 |
|---|---|---|---|
| **Chatbot** | 人 | 一问一答 | ChatGPT 早期版本 |
| **Workflow** | 开发者（写死） | 预定义步骤 | n8n、Zapier、LangChain Chain |
| **Agent** | LLM 自己 | 动态循环 | Claude Code、Cursor Agent、Devin |

**关键判据**：控制流是**硬编码**的还是 **LLM 动态决定**的？

> 引用 Anthropic 的定义：
> *"Workflows are systems where LLMs and tools are orchestrated through predefined code paths. Agents, on the other hand, are systems where LLMs dynamically direct their own processes and tool usage."*

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

## 1.5 AI Agent 的典型应用形态

| 形态 | 代表产品 | 核心工具 |
|---|---|---|
| 编码助手 | Claude Code, Cursor, Aider | 文件读写、shell、grep |
| 浏览器 Agent | Browser Use, Anthropic Computer Use | 点击、输入、截图 |
| 研究助手 | Perplexity, OpenAI Deep Research | 搜索、阅读网页 |
| 数据 Agent | Julius, Hex Magic | SQL、Python 执行 |
| 运维 Agent | Cleric | 日志查询、监控、Runbook |

本书的实战项目 **mini-claude-code** 属于第一类——最成熟、最赚钱、最适合拿来当求职作品集。

## 1.6 小结

- Agent = LLM + 工具 + 循环决策
- 与 Workflow 的区别是控制流由谁决定
- 最小闭环是 **Agent Loop**，后面所有内容都是在丰富这个循环

> **下一章**：我们补上"LLM 到底是什么"这一课，让非算法背景的读者也能踏实地继续。

