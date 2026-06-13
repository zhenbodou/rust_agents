"""LangGraph 适配器（书第 44 章 —— 对接已有 LangGraph Agent）。

架构：
  LangGraph ReAct 图（model_node ↔ tool_node 循环）→ 流式事件
  → 转 TraceEvent 流上报到 eval-server。

依赖：
  uv sync --extra langgraph
  (langgraph>=0.2, langchain-anthropic / langchain-openai, 任选其一)

工具只暴露安全的 bash（临时目录，同 AnthropicAdapter）。
切换底层 LLM：设置 LG_MODEL_PROVIDER=openai|anthropic（默认 anthropic）。
"""

from __future__ import annotations

import asyncio
import os
import tempfile
import time
import uuid
from collections.abc import AsyncIterator
from typing import Any

from runner.adapters.base import RunRequest
from runner.events import (
    LlmChunk,
    LlmRequest,
    RunFinished,
    RunStarted,
    Seq,
    ToolCall,
    ToolResult,
    TraceEvent,
)

PRICE_IN, PRICE_OUT = 3e-6, 15e-6  # 与 anthropic_agent.py 对齐，生产应按模型查表


class LangGraphAdapter:
    """将 LangGraph ReAct agent 包装成标准 AgentAdapter。"""

    def __init__(self) -> None:
        # 惰性导入：未安装 extra 时其他 scaffold 不受影响
        self._validate_deps()

    def _validate_deps(self) -> None:
        try:
            import langgraph  # noqa: F401
        except ImportError:
            raise ImportError(
                "langgraph not installed. Run: uv sync --extra langgraph"
            )

    async def run(self, req: RunRequest) -> AsyncIterator[TraceEvent]:
        import langgraph.prebuilt  # noqa: F401
        from langchain_core.messages import AIMessage, HumanMessage, ToolMessage
        from langchain_core.tools import tool as lc_tool
        from langgraph.prebuilt import create_react_agent

        seq = Seq()
        yield RunStarted(
            seq=seq.next(), run_id=req.run_id,
            case_id=req.case_id, task=req.task, model=req.model,
        )

        with tempfile.TemporaryDirectory() as workspace:
            # ── 工具定义 ──────────────────────────────────────────────────
            @lc_tool
            async def bash(command: str) -> str:
                """Run a shell command in the workspace directory."""
                out, _ = await _run_bash(command, cwd=workspace)
                return out

            # ── LLM 选择 ──────────────────────────────────────────────────
            llm = _build_llm(req.model)

            # ── ReAct 图 ──────────────────────────────────────────────────
            graph = create_react_agent(llm, tools=[bash])

            in_tok = out_tok = 0
            turn = 0
            text_buf: list[str] = []

            try:
                # LangGraph 流式 API：stream(input, stream_mode="updates")
                async for chunk in graph.astream(
                    {"messages": [HumanMessage(content=req.task)]},
                    stream_mode="updates",
                    config={"recursion_limit": req.max_turns * 2},
                ):
                    # chunk = { node_name: { messages: [...] } }
                    for node_name, node_output in chunk.items():
                        msgs = node_output.get("messages", [])
                        for msg in msgs:
                            if isinstance(msg, AIMessage):
                                turn += 1
                                # 记录 token 用量（langchain AIMessage.usage_metadata）
                                usage = getattr(msg, "usage_metadata", None) or {}
                                in_tok += usage.get("input_tokens", 0)
                                out_tok += usage.get("output_tokens", 0)
                                yield LlmRequest(
                                    seq=seq.next(), turn=turn,
                                    input_tokens=in_tok,
                                )
                                # 文本内容
                                text = msg.content if isinstance(msg.content, str) else ""
                                if text:
                                    text_buf.append(text)
                                    yield LlmChunk(
                                        seq=seq.next(), turn=turn, delta=text
                                    )
                                # 工具调用
                                for tc in (msg.tool_calls or []):
                                    call_id = tc.get("id", uuid.uuid4().hex[:8])
                                    yield ToolCall(
                                        seq=seq.next(), turn=turn,
                                        call_id=call_id,
                                        tool_name=tc["name"],
                                        args=tc.get("args"),
                                    )

                            elif isinstance(msg, ToolMessage):
                                yield ToolResult(
                                    seq=seq.next(),
                                    call_id=msg.tool_call_id or uuid.uuid4().hex[:8],
                                    output=str(msg.content)[:10_000],
                                    is_error=False,
                                    duration_ms=0,
                                )

            except Exception as exc:  # graph 抛异常（超过 recursion_limit 等）
                yield RunFinished(
                    seq=seq.next(), status="error",
                    cost_usd=in_tok * PRICE_IN + out_tok * PRICE_OUT,
                    turns=turn,
                )
                return

            yield RunFinished(
                seq=seq.next(), status="passed",
                score=None,  # 由 scoring 模块在 main.py 里算
                cost_usd=in_tok * PRICE_IN + out_tok * PRICE_OUT,
                turns=turn,
            )


def _build_llm(model: str) -> Any:
    """按模型名前缀选 LangChain LLM 实现。"""
    provider = os.environ.get("LG_MODEL_PROVIDER", "anthropic")
    if provider == "openai":
        from langchain_openai import ChatOpenAI  # type: ignore
        return ChatOpenAI(model=model, streaming=True)
    else:
        from langchain_anthropic import ChatAnthropic  # type: ignore
        return ChatAnthropic(model_name=model, streaming=True)


async def _run_bash(command: str, cwd: str, timeout_s: int = 60) -> tuple[str, bool]:
    try:
        proc = await asyncio.create_subprocess_shell(
            command, cwd=cwd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.STDOUT,
        )
        out, _ = await asyncio.wait_for(proc.communicate(), timeout=timeout_s)
        return out.decode(errors="replace"), proc.returncode != 0
    except TimeoutError:
        try:
            proc.kill()
        except ProcessLookupError:
            pass
        return f"timeout after {timeout_s}s", True
