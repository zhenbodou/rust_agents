"""Runner 主循环：lease → 跑 adapter → 批量上报事件 → complete（书第 50 章）。

生产改进（相比教学版）：
- Bearer token 鉴权（RUNNER_SECRET）
- 运行时长计量（墙钟时间）上报给 /complete
- 集成评分模块（scoring.py）：有 expectations 时计算 score / status
- LangGraph scaffold 支持
"""

from __future__ import annotations

import asyncio
import os
import socket
import sys
import time
from typing import Any

import httpx

from runner.adapters.base import AgentAdapter, RunRequest
from runner.adapters.mock import MockAdapter
from runner.events import LlmChunk, RunFinished, Seq, ToolCall, ToolResult
from runner.scoring import TraceSnapshot, score as compute_score

SERVER = os.environ.get("EVAL_SERVER_URL", "http://localhost:8080")
RUNNER_ID = os.environ.get("RUNNER_ID", f"{socket.gethostname()}-{os.getpid()}")
RUNNER_SECRET = os.environ.get("RUNNER_SECRET")  # None → dev 模式（服务端放行）
POLL_IDLE_S = float(os.environ.get("POLL_IDLE_S", "2.0"))
FLUSH_INTERVAL_S = 0.5


def _auth_headers() -> dict[str, str]:
    if RUNNER_SECRET:
        return {"Authorization": f"Bearer {RUNNER_SECRET}"}
    return {}


def make_adapter(scaffold: str) -> AgentAdapter:
    if scaffold == "mock":
        return MockAdapter()
    if scaffold == "anthropic":
        from runner.adapters.anthropic_agent import AnthropicAdapter
        return AnthropicAdapter()
    if scaffold == "langgraph":
        from runner.adapters.langgraph_agent import LangGraphAdapter
        return LangGraphAdapter()
    raise ValueError(f"unknown scaffold: {scaffold}")


async def heartbeat_loop(http: httpx.AsyncClient, run_id: str) -> None:
    while True:
        await asyncio.sleep(30)
        r = await http.post(
            f"/internal/runs/{run_id}/heartbeat",
            json={"runner_id": RUNNER_ID},
        )
        if not r.json().get("alive"):
            # 租约被 reaper 回收（多半是我们卡太久）
            raise RuntimeError("lease lost")


async def execute(http: httpx.AsyncClient, lease: dict) -> None:
    run_id = lease["run_id"]
    expectations: list[dict] = lease.get("expectations") or []
    req = RunRequest(
        run_id=run_id, case_id=lease["case_id"],
        task=lease["task"], model=lease["model"],
    )
    adapter = make_adapter(lease["scaffold"])

    buf: list[str] = []
    snap = TraceSnapshot()
    final: RunFinished | None = None
    t_start = time.monotonic()

    async def flush() -> None:
        if buf:
            lines, buf[:] = "\n".join(buf), []
            await http.post(f"/internal/runs/{run_id}/events", content=lines)

    hb = asyncio.create_task(heartbeat_loop(http, run_id))
    try:
        last_flush = asyncio.get_event_loop().time()
        async for ev in adapter.run(req):
            # 构建 TraceSnapshot（供评分用）
            if isinstance(ev, ToolCall):
                snap.tool_calls.append(ev)
            elif isinstance(ev, ToolResult):
                snap.tool_results.append(ev)
                if ev.is_error:
                    snap.had_error = True
            elif isinstance(ev, LlmChunk):
                snap.text_outputs.append(ev.delta)
            elif isinstance(ev, RunFinished):
                final = ev

            buf.append(ev.model_dump_json())
            now = asyncio.get_event_loop().time()
            if now - last_flush >= FLUSH_INTERVAL_S:
                await flush()
                last_flush = now

        await flush()
        duration_s = time.monotonic() - t_start

        # ── 评分 ───────────────────────────────────────────────────────
        # adapter 返回的 RunFinished.status 是执行状态（passed/error/timeout）；
        # 如果有 expectations，覆盖 status 和 score。
        if final and final.status in ("passed", "failed") and expectations:
            result = compute_score(snap, expectations)
            status = result.status
            score_val = result.score
        elif final:
            status = final.status
            score_val = final.score
        else:
            status = "error"
            score_val = None

        await http.post(f"/internal/runs/{run_id}/complete", json={
            "runner_id": RUNNER_ID,
            "status": status,
            "score": score_val,
            "cost_usd": final.cost_usd if final else None,
            "turns": final.turns if final else None,
            "duration_s": round(duration_s, 3),
            "error": None,
        })
        print(f"[{RUNNER_ID}] run {run_id} -> {status}"
              + (f" score={score_val:.3f}" if score_val is not None else ""))

    except Exception as e:  # 单 run 失败隔离（ch41 rollout 骨架的铁律）
        duration_s = time.monotonic() - t_start
        print(f"[{RUNNER_ID}] run {run_id} crashed: {e}", file=sys.stderr)
        try:
            await flush()
            await http.post(f"/internal/runs/{run_id}/complete", json={
                "runner_id": RUNNER_ID, "status": "error",
                "error": str(e)[:1000], "duration_s": round(duration_s, 3),
            })
        except Exception:
            pass  # complete 也失败：交给租约过期 + reaper
    finally:
        hb.cancel()


async def main() -> None:
    headers = _auth_headers()
    if RUNNER_SECRET:
        print(f"[{RUNNER_ID}] auth: Bearer token configured")
    else:
        print(f"[{RUNNER_ID}] auth: none (dev mode)")
    print(f"[{RUNNER_ID}] polling {SERVER}")

    async with httpx.AsyncClient(
        base_url=SERVER,
        headers=headers,
        timeout=httpx.Timeout(10, read=120),
    ) as http:
        while True:
            try:
                r = await http.post("/internal/lease", json={"runner_id": RUNNER_ID})
                r.raise_for_status()
                lease = r.json().get("run")
            except Exception as e:
                print(f"[{RUNNER_ID}] lease failed ({e}); retrying", file=sys.stderr)
                await asyncio.sleep(POLL_IDLE_S)
                continue

            if lease is None:
                await asyncio.sleep(POLL_IDLE_S)
                continue
            await execute(http, lease)


if __name__ == "__main__":
    asyncio.run(main())
