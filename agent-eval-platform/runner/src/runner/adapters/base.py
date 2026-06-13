"""统一适配协议（书第 42 章：以 TraceEvent 流为契约，scaffold 随便换）。"""

from __future__ import annotations

from collections.abc import AsyncIterator
from typing import Protocol

from pydantic import BaseModel

from runner.events import TraceEvent


class RunRequest(BaseModel):
    run_id: str
    case_id: str
    task: str
    model: str
    max_turns: int = 20


class AgentAdapter(Protocol):
    """所有 scaffold 适配器实现这一个方法：执行任务，流式产出标准轨迹事件。"""

    def run(self, req: RunRequest) -> AsyncIterator[TraceEvent]: ...
