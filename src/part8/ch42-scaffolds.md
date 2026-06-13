# 第 42 章 对接 Agent 框架：LangChain / LangGraph / OpenAI Agents SDK

> 前面你都是从零手写 Agent。但业界有很多现成的"Agent 框架"（也叫 scaffold，脚手架），算法同学常用它们快速搭 Agent。Harness 工程师的一个核心职责，就是让这些五花八门的框架都能接入公司的训练和评测平台。本章先讲清"框架是什么、为什么存在"，再做这件真正的工程活——设计一个统一适配层。

## 42.1 先理解：框架是什么、为什么有这么多

你 Part 1–5 手写的那个 Agent Loop，框架们也都有——**所有框架的内核，都是第 7 章那个"请求→执行工具→循环"的循环**。区别只在于它们各自的"封装风格"和附加功能。

打个比方：自己手写 Agent 像自己买零件组装电脑（完全可控但费劲），用框架像买品牌整机（开箱即用但要适应它的设计）。主流的几个：

| 框架 | 它是什么 | 适合 |
|---|---|---|
| **LangChain** | 一个大组件库，把"模型/工具/检索"统一成接口 | 快速拼装、生态最大 |
| **LangGraph** | 把 Agent 画成"状态图"，每步可暂停/恢复 | 复杂流程、要人工介入 |
| **OpenAI Agents SDK** | 轻量的多 Agent 原语 | OpenAI 生态、多 Agent 协作 |
| 自研（你的 mini-claude-code） | 全部自己掌控 | 学习、特殊需求、RL 集成 |

**关键认知**：理解了内核都是同一个 loop，你看任何新框架都能在 30 分钟内上手——因为你知道该去找它的"循环在哪、工具怎么定义、状态怎么存"。下面快速过一遍每个，不求记住 API，只求建立"它长什么样"的印象。

## 42.2 LangChain：组件库

```python
# uv add langchain langchain-anthropic
from langchain_anthropic import ChatAnthropic
from langchain_core.tools import tool

@tool
def read_file(path: str) -> str:
    """读取文件内容。"""           # 这段文档会被框架拿去告诉模型这个工具干嘛
    return open(path).read()[:8000]

llm = ChatAnthropic(model="claude-sonnet-4-6")
llm_with_tools = llm.bind_tools([read_file])    # 把工具绑给模型
```

LangChain 的**好处**是统一接口——想换个模型厂商只改一行。**代价**是抽象层很深，出问题时要扒好几层源码。所以很多生产团队只用它"统一调模型"这一层，循环还是自己写。

## 42.3 LangGraph：把 Agent 画成状态图

LangGraph 让你把 Agent 显式画成一张"流程图"——有哪些步骤（节点）、怎么流转（边）。最大的好处是**每一步的状态都能存档**，于是可以崩溃恢复、可以中途暂停等人审批：

```python
from langgraph.graph import StateGraph, END

# 定义图：模型节点 ↔ 工具节点 来回流转
g = StateGraph(AgentState)
g.add_node("model", call_model)        # 调模型这一步
g.add_node("tools", call_tools)        # 执行工具这一步
g.set_entry_point("model")
g.add_conditional_edges("model", should_continue, {"tools": "tools", "end": END})
g.add_edge("tools", "model")           # 工具执行完回到模型

# 编译时挂上"存档器"，并设置"执行工具前先暂停"
app = g.compile(checkpointer=saver, interrupt_before=["tools"])
```

注意那个"执行工具前暂停等人审批"——这和你第 11 章手写的权限系统，是**同一个问题的两种解法**：你用代码里的 Decision::Ask 实现，框架把它做成了图的暂停/恢复。看到这种对应关系，说明你真的理解了背后的原理。

## 42.4 OpenAI Agents SDK：多 Agent 协作

这个框架擅长"多个 Agent 配合干活"。比如一个写代码的 Agent 写完后，把活"移交"给一个审查代码的 Agent：

```python
from agents import Agent, Runner, handoff

reviewer = Agent(name="reviewer", instructions="审查代码有没有 bug 和风格问题。")
coder = Agent(
    name="coder",
    instructions="实现需求，然后移交给 reviewer。",
    handoffs=[handoff(reviewer)],      # handoff = 把控制权交给另一个 Agent
)
result = await Runner.run(coder, "给 login() 加输入校验")
```

它有三个设计值得借鉴进自研 harness：**Handoff**（Agent 间移交控制权，是第 14 章 subagent"调用-返回"之外的另一种协作拓扑）、**Guardrail**（输入/输出校验器，不合规就熔断）、**Session**（自动管理多轮对话历史）。

## 42.5 核心实战：设计统一适配层

现在做这个岗位真正要的活。**场景**：公司里算法同学用的框架五花八门，但评测平台和 RL 训练只想用一套标准接口。你的任务是设计一个"统一适配层"，让任何框架都能低成本接入。

**核心思路**：定义一个"最小公共协议"，给每个框架写一个"适配器"把它翻译到这个协议上。就像各国插头不同，你提供一个万能转换插座。

那么"公共协议"是什么？答案是——**统一的轨迹事件流**（第 41 章定义的 TraceEvent）。不管底层用什么框架，都要吐出同样格式的事件：

```python
from typing import Protocol, AsyncIterator

class UnifiedAgent(Protocol):
    """所有框架的适配器都要实现这个接口——平台的接入契约。"""
    async def run(self, req: AgentRunRequest) -> AsyncIterator[TraceEvent]:
        """执行任务，流式产出标准化的轨迹事件。"""
        ...
```

这里有三个值得讲十分钟的设计决策（面试设计题）：

1. **用"事件流"当契约，而不是只给最终结果**——因为评测平台要回放过程、RL 训练要过程数据，光有结果不够；
2. **工具执行收归平台统一的沙箱**——否则每个框架自带工具，行为不一致、不可控、不安全；
3. **事件模型要版本化**——三个团队都依赖它，改动要能平滑灰度。

一个适配器长这样（把 LangGraph 的事件翻译成标准 TraceEvent）：

```python
class LangGraphAdapter:
    async def run(self, req) -> AsyncIterator[TraceEvent]:
        async for ev in self.graph.astream_events(...):
            match ev["event"]:
                case "on_chat_model_stream":
                    yield LlmChunkEvent(delta=ev["data"]["chunk"].content, ...)
                case "on_tool_start":
                    yield ToolCallEvent(tool_name=ev["name"], args=ev["data"]["input"], ...)
                case "on_tool_end":
                    yield ToolResultEvent(output=str(ev["data"]["output"]), ...)
```

OpenAI Agents SDK 用它自己的流式事件做同样翻译，你的自研 Rust Agent 天生就输出这个格式。**平台只认 TraceEvent，上游随便换**——这就是"降低接入成本"的工程含义。

## 42.6 用 MCP 统一工具

工具收归平台后，怎么把同一套工具喂给不同框架？答案是第 14a 章的 **MCP 协议**——它正在成为跨框架的工具标准。让沙箱暴露一个 MCP 服务，所有框架通过各自的 MCP 客户端用同一套工具：

```python
# LangChain 这边
tools = await MultiServerMCPClient({"sandbox": {"url": "http://sandbox:9000/mcp"}}).get_tools()

# OpenAI Agents SDK 那边
agent = Agent(name="coder", mcp_servers=[MCPServerStreamableHttp(params={"url": "http://sandbox:9000/mcp"})])
```

于是平台架构收敛成：**工具实现一份（在 MCP 服务里），鉴权、配额、审计也都在那一处做，所有框架共用**。

## 42.7 怎么选框架（面试题）

```
要完全控制循环 / 深度定制 / RL 集成？  → 自研（Rust 或 Python）
要复杂编排 + 存档 + 人工介入？          → LangGraph
多 Agent 移交 + OpenAI 生态？           → OpenAI Agents SDK
只是想统一调各家模型？                  → 只用 langchain-core，或直接用官方 SDK
```

一个成熟的态度：**别把框架当宗教，也别当瘟疫**。按层取用——调模型这层用框架省事，循环和 harness 这层自己掌控。

## 42.8 小结与练习

- 所有框架的内核都是第 7 章那个 loop + 不同封装；懂了这点看任何新框架都快。
- 平台集成的正解：统一的 TraceEvent 事件流当契约 + 每个框架一个适配器 + 用 MCP 统一工具层。
- 工具执行必须收归平台沙箱，否则不可比、不可控、不安全。

**练习**

1. 用 LangGraph 重建 mini-claude-code 的主循环（含"工具前暂停审批"），和你手写的权限系统对比。
2. 写一个适配器把某个框架的运行轨迹翻译成 TraceEvent，让它能在第 38 章的轨迹查看器里回放。
3. 把 mini-claude-code 的 Read/Bash 工具包成一个 MCP 服务，分别从 LangChain 和 Agents SDK 调用成功。

> **下一章**：Agent × 强化学习（RL）——这是离"模型训练"最近的一章。你会看到你前面造的所有 harness，正是 RL 训练需要的"环境"。
