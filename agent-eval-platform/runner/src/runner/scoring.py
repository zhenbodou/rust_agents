"""真实评分器：根据 expectations 对 trace 打分（书第 43 章）。

评分策略分三层（按权重叠加）：
  1. tool_sequence  工具调用序列断言（顺序 / 无序，支持正则）
  2. output_contains 最终输出文本断言（大小写不敏感 substring / regex）
  3. no_errors      执行无错误奖励

期望格式（batches.cases[i].expectations）：

    [
      { "type": "tool_called", "tool": "bash",
        "args_regex": "pytest", "weight": 0.4 },
      { "type": "output_contains", "pattern": "passed",
        "weight": 0.4 },
      { "type": "no_tool_errors", "weight": 0.2 }
    ]

未配置 expectations → 固定通过，score = None（由人工 review）。

输出：
  status: "passed" | "failed"
  score:  0.0 – 1.0  (None if no expectations)
  reason: 简短说明字符串
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import Any

from runner.events import RunFinished, ToolCall, ToolResult


@dataclass
class TraceSnapshot:
    """从事件流里提取的 run 摘要，传给 score()。"""
    tool_calls: list[ToolCall] = field(default_factory=list)
    tool_results: list[ToolResult] = field(default_factory=list)
    text_outputs: list[str] = field(default_factory=list)  # 所有 LlmChunk.delta 的拼接
    had_error: bool = False  # 任意 ToolResult.is_error


@dataclass
class ScoreResult:
    status: str          # "passed" | "failed"
    score: float | None  # None = 没有 expectations
    reason: str


def score(snapshot: TraceSnapshot, expectations: list[dict[str, Any]]) -> ScoreResult:
    """评分入口。"""
    if not expectations:
        # 没有 expectations：运行完成即视为 passed，score 留给人工 review
        return ScoreResult(status="passed", score=None, reason="no expectations; manual review")

    total_weight = 0.0
    earned = 0.0
    reasons: list[str] = []

    for exp in expectations:
        etype = exp.get("type", "")
        weight = float(exp.get("weight", 1.0))
        total_weight += weight

        if etype == "tool_called":
            ok, why = _check_tool_called(snapshot, exp)
        elif etype == "output_contains":
            ok, why = _check_output_contains(snapshot, exp)
        elif etype == "no_tool_errors":
            ok, why = not snapshot.had_error, (
                "no tool errors" if not snapshot.had_error else "had tool errors"
            )
        elif etype == "tool_call_count":
            ok, why = _check_tool_count(snapshot, exp)
        else:
            # 未知 expectation 类型：跳过（宽松），不影响得分
            total_weight -= weight
            continue

        if ok:
            earned += weight
            reasons.append(f"✓ {etype}: {why}")
        else:
            reasons.append(f"✗ {etype}: {why}")

    if total_weight == 0:
        return ScoreResult(status="passed", score=None, reason="no scorable expectations")

    ratio = earned / total_weight
    # 阈值：0.6 及以上视为 passed（可通过 expectation 里加 threshold 覆盖）
    threshold = float(expectations[0].get("pass_threshold", 0.6)) if expectations else 0.6
    status = "passed" if ratio >= threshold else "failed"
    return ScoreResult(status=status, score=round(ratio, 4),
                       reason="; ".join(reasons) or "ok")


# ─── 断言实现 ─────────────────────────────────────────────────────────────

def _check_tool_called(snap: TraceSnapshot, exp: dict) -> tuple[bool, str]:
    """验证某工具是否被调用（且参数可选匹配正则）。"""
    tool_name = exp.get("tool", "")
    args_regex = exp.get("args_regex")
    ordered_after = exp.get("after_tool")  # 要求在 after_tool 之后调用

    matched: list[ToolCall] = [
        tc for tc in snap.tool_calls if tc.tool_name == tool_name
    ]
    if not matched:
        return False, f"tool '{tool_name}' not called"

    if args_regex:
        pattern = re.compile(args_regex, re.IGNORECASE)
        matched = [
            tc for tc in matched
            if pattern.search(str(tc.args or ""))
        ]
        if not matched:
            return False, f"tool '{tool_name}' called but args did not match /{args_regex}/"

    if ordered_after:
        # 检查 after_tool 在 matched[0] 之前存在
        after_idxs = [i for i, tc in enumerate(snap.tool_calls)
                      if tc.tool_name == ordered_after]
        first_match_idx = snap.tool_calls.index(matched[0])
        if not any(i < first_match_idx for i in after_idxs):
            return False, f"tool '{tool_name}' not called after '{ordered_after}'"

    return True, f"tool '{tool_name}' called {len(matched)} time(s)"


def _check_output_contains(snap: TraceSnapshot, exp: dict) -> tuple[bool, str]:
    """验证文本输出包含某 substring 或匹配某正则。"""
    pattern_str = exp.get("pattern", "")
    use_regex = exp.get("regex", False)
    full_text = " ".join(snap.text_outputs)

    if use_regex:
        ok = bool(re.search(pattern_str, full_text, re.IGNORECASE))
        return ok, f"regex /{pattern_str}/ {'found' if ok else 'not found'}"
    else:
        ok = pattern_str.lower() in full_text.lower()
        return ok, f"'{pattern_str}' {'found' if ok else 'not found'} in output"


def _check_tool_count(snap: TraceSnapshot, exp: dict) -> tuple[bool, str]:
    """验证某工具调用次数在 [min, max] 范围内。"""
    tool_name = exp.get("tool", "")
    min_c = exp.get("min", 0)
    max_c = exp.get("max", 999_999)
    count = sum(1 for tc in snap.tool_calls if tc.tool_name == tool_name)
    ok = min_c <= count <= max_c
    return ok, f"tool '{tool_name}' called {count} time(s) (expected [{min_c}, {max_c}])"
