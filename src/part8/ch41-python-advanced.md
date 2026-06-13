# 第 41 章 Python 进阶：类型标注、数据校验与异步并发

> 上一章你能写完整的 Python 程序了。本章升级到"工程级"，补三样写 Agent 后端绕不开的装备：给 Python 加类型标注（让编辑器帮你抓错）、用 pydantic 校验外部数据、用 asyncio 同时处理很多任务。每一样都先讲清楚"为什么需要它"。

## 41.1 类型标注：给 Python 也装上"安全带"

上一章说 Python 是"动态类型"——变量随便装什么都行。这灵活，但和第 34 章讲 JS 时一样的问题：很多错误要到运行时才暴露。Python 的解法和 TypeScript 一模一样——**加类型标注**，让编辑器和检查工具在你写错时当场提醒：

```python
# 在参数和返回值上标类型
def total_ms(events: list[dict], only_ok: bool = False) -> int:
    ...

count: int = 0
scores: dict[str, float] = {}
maybe: str | None = None        # "可能是字符串，也可能是空"
```

标注不会影响程序运行（Python 不强制），但配合检查工具 **pyright**（VS Code 的 Python 插件内核）就能在编辑器里全程帮你查错。规则和 TS 一致：内部代码靠类型标注护航，**外部进来的数据要单独校验**（下一节）。

> 顺带给你一张三门语言的"工具对照表"（学过前面就有亲切感，没学过忽略即可）：包管理 Rust 用 cargo、JS 用 pnpm、Python 用 **uv**；类型检查分别是编译器自带、tsc、**pyright**；数据校验是 serde、Zod、**pydantic**。概念全相通，只是名字不同。

项目初始化和质量工具一次配好：

```bash
uv init agent-rollout --python 3.12
cd agent-rollout
uv add pydantic httpx anthropic              # 运行依赖
uv add --dev pytest pytest-asyncio ruff pyright   # 开发工具：测试、lint、类型检查
```

`ruff` 是个二合一工具（既查代码风格又自动格式化，极快），`pyright` 查类型。把它们配进 `pyproject.toml`，提交前跑一遍，代码质量就有了底线。

## 41.2 pydantic：守住"数据入口"

回忆第 34 章讲 Zod 时的道理：从外部进来的数据（后端响应、读进来的文件）是不可信的，必须在入口校验。Python 里这件事的标准工具是 **pydantic**——你定义数据"应该长什么样"，它负责检查真实数据合不合规，不合规就报错并告诉你哪里错了。

```python
from typing import Literal
from pydantic import BaseModel

# 定义一个事件的模型：继承 BaseModel，把字段和类型写出来
class ToolCallEvent(BaseModel):
    type: Literal["tool_call"] = "tool_call"   # Literal 表示这个字段只能是这个固定值
    turn: int
    tool_name: str
    args: dict
    ts: float

# 用它校验数据：不合规会抛出带详细信息的错误
event = ToolCallEvent.model_validate_json('{"turn":1,"tool_name":"bash","args":{},"ts":1.0}')
print(event.tool_name)   # "bash"，而且类型确定，编辑器有提示
```

pydantic 也支持"判别联合"（同一个字段区分多种事件形状），和第 34 章 Zod、Part 5 的 Rust enum 是同一个概念：

```python
from typing import Annotated, Union
from pydantic import Field, TypeAdapter

# 用 type 字段区分是哪种事件
TraceEvent = Annotated[
    Union[ToolCallEvent, ToolResultEvent, RunFinishedEvent],
    Field(discriminator="type"),
]

adapter = TypeAdapter(TraceEvent)
event = adapter.validate_python(raw_dict)   # 自动按 type 选对正确的模型校验
```

> **一个 Harness 工程师的日常**：同一个"事件模型"往往要在 Rust（serde）、TypeScript（Zod）、Python（pydantic）三处保持完全一致，否则三端对不上就出 bug。生产做法是用一份 JSON Schema 当"唯一真相"，自动生成三种语言的代码（第 49 章实战）。

## 41.3 asyncio：同时处理成百上千个任务

这是本章最实用的部分。回到第 33 章那个"独臂服务员"的类比——一个人也能高效服务很多桌，靠的是"点完这桌的菜不傻等，转身去服务下一桌"。Python 的 `asyncio` 就是让你这样写代码：发起一个耗时操作（比如调 LLM）后不干等，转去处理别的，结果回来了再继续。

为什么 Agent 后端特别需要它？因为评测/数据采集要**同时跑几百个 Agent**，每个大部分时间都在等 LLM 响应。用 asyncio，一个进程就能同时盯着几百个，效率极高。

基本语法（和第 33 章 JS 的 async/await 几乎一样）：

```python
import asyncio

async def fetch_one(case):          # async def 定义异步函数
    result = await call_llm(case)   # await 等一个耗时操作，期间让出去干别的
    return result

# 同时启动很多个、一起等（关键：这是并发，不是一个个排队）
results = await asyncio.gather(*(fetch_one(c) for c in cases))
```

下面是一个**生产级的批量执行骨架**，是所有"批量跑 LLM 任务"的通用模板，三个要点都标注了：

```python
import asyncio
from anthropic import AsyncAnthropic

class RolloutRunner:
    def __init__(self, concurrency: int = 16):
        self.client = AsyncAnthropic()
        self.sem = asyncio.Semaphore(concurrency)   # 信号量：限制同时最多 16 个

    async def run_one(self, case):
        async with self.sem:                         # 要点1：限流，别一下发几百个把 API 打挂
            try:
                async with asyncio.timeout(300):     # 要点2：单个任务硬超时 5 分钟
                    return await self._agent_loop(case)
            except TimeoutError:
                return Result(case.id, "timeout")
            except Exception as e:                   # 要点3：单个崩了不能炸掉整批
                return Result(case.id, "error", str(e))

    async def run_all(self, cases):
        tasks = [asyncio.create_task(self.run_one(c)) for c in cases]
        results = []
        for fut in asyncio.as_completed(tasks):      # 谁先完成先处理，进度实时可见
            r = await fut
            print(f"[{len(results)+1}/{len(cases)}] {r.case_id}: {r.status}")
            results.append(r)
        return results
```

记住这三条——**限流（Semaphore）、超时、单任务异常隔离**——它们是所有批量 LLM 任务的"标准三件套"，面试和实战都用得上。

## 41.4 用 Python 重写 Agent Loop

把第 7 章的 Rust Agent 主循环用 Python 写一遍。面试官常用"换种语言再讲一遍"来确认你懂的是**概念**而非某种语法：

```python
from anthropic import AsyncAnthropic

async def agent_loop(task: str, max_turns: int = 20) -> str:
    client = AsyncAnthropic()
    messages = [{"role": "user", "content": task}]

    for _ in range(max_turns):
        resp = await client.messages.create(
            model="claude-sonnet-4-6", max_tokens=4096,
            system="你是编码 Agent，用 bash 工具完成任务。",
            tools=TOOLS, messages=messages,
        )
        messages.append({"role": "assistant", "content": resp.content})

        if resp.stop_reason != "tool_use":          # 不需要工具了 → 结束
            return get_text(resp)

        # 执行模型要求的每个工具，把结果回填
        tool_results = []
        for block in resp.content:
            if block.type == "tool_use":
                output = await run_bash(**block.input)
                tool_results.append({
                    "type": "tool_result", "tool_use_id": block.id, "content": output,
                })
        messages.append({"role": "user", "content": tool_results})

    return "超过最大轮数"
```

对照 Part 5 的 Rust 版，你会发现**结构一模一样**：发请求 → 看 stop_reason → 执行工具 → 回填结果 → 循环。**语言只是表皮，Agent 的结构才是本质**——你 Part 1–5 学的东西在任何语言里都成立。

## 41.5 让 Python 和你的 Rust 服务对话

Harness 工程师常见任务：算法团队的 Python 代码要调用你写的 Rust Agent 服务。三种方式，按"耦合松紧"排序：

1. **HTTP API（首选，最松）**：Rust 起一个 HTTP 服务，Python 用 `httpx` 库去调。跨机器、跨语言都行，是第 43 章 rollout 链路用的方式。
2. **子进程 + 标准输入输出（零部署）**：Python 把 Rust 程序当子进程拉起来，通过它的输入输出管道传任务和事件。
3. **PyO3 原生绑定（最紧）**：把 Rust 编译成一个 Python 能直接 `import` 的模块，适合高频调用的性能热点（如轨迹解析）。

```python
import httpx

class AgentClient:
    def __init__(self, base_url: str):
        # 连接超时 10 秒，但读取（等 Agent 跑完）给 600 秒
        self._http = httpx.AsyncClient(base_url=base_url, timeout=httpx.Timeout(10, read=600))

    async def submit_run(self, task: str) -> str:
        r = await self._http.post("/api/runs", json={"task": task})
        r.raise_for_status()
        return r.json()["run_id"]
```

## 41.6 测试

Agent 代码测试不能真打 LLM（慢、贵、结果不确定），要用"假对象"（mock）替身。pytest 配 `AsyncMock` 即可：

```python
import pytest
from unittest.mock import AsyncMock

async def test_agent_stops_on_text_response():
    client = AsyncMock()
    client.messages.create.return_value = make_text_response("完成")  # 假装 LLM 回了"完成"
    result = await agent_loop_with(client, "做点事")
    assert result == "完成"
    assert client.messages.create.await_count == 1   # 确认只调了一次

async def test_one_failure_does_not_break_batch():
    runner = RolloutRunner(concurrency=2)
    results = await runner.run_all([good_case, crashing_case])
    assert {r.status for r in results} == {"success", "error"}   # 一个崩另一个照常
```

（第 43 章补充会把 pytest 讲到精通：fixture、参数化、覆盖率门禁。）

## 41.7 小结与练习

- 给 Python 加类型标注 + pyright，效果等同 TS：写错当场提醒；外部数据用 pydantic 在入口校验。
- pydantic 的判别联合 = Zod = Rust enum，三语对齐靠一份 schema 当唯一真相。
- asyncio 让一个进程并发处理成百上千任务；批量 LLM 任务的标准三件套：限流、超时、单任务异常隔离。
- Agent Loop 换成 Python 结构不变——语言是表皮，结构是本质。

**练习**

1. 用 Python 重写第 18 章的评测 Runner，跑通同一份 YAML 数据集，对比两版代码结构。
2. 写一个 `trace_stats.py`：用 pydantic 校验 mini-claude-code 的 JSONL session，输出每个工具的调用次数、P50/P99 耗时、总成本。
3. 实现"子进程互操作"：用 Python 拉起 mini-claude-code 子进程完成一个编码任务，并实时打印它产生的事件。

> **下一章**：对接各种 Agent 框架（LangChain、LangGraph、OpenAI Agents SDK），并设计一个让它们都能接入的统一适配层。
