"""三端契约测试：事件序列化必须与 schemas/trace-event.schema.json 一致。"""

import json

import pytest
from pydantic import ValidationError

from runner.events import RunFinished, Seq, ToolCall, trace_event_adapter


def test_tool_call_round_trip():
    seq = Seq()
    ev = ToolCall(seq=seq.next(), turn=1, call_id="c1", tool_name="bash", args={"command": "ls"})
    raw = json.loads(ev.model_dump_json())
    assert raw["type"] == "tool_call"
    assert raw["schema_version"] == 1
    back = trace_event_adapter.validate_python(raw)
    assert isinstance(back, ToolCall)


def test_unknown_type_rejected():
    with pytest.raises(ValidationError):
        trace_event_adapter.validate_python(
            {"schema_version": 1, "seq": 0, "ts": 0, "type": "mystery"}
        )


def test_seq_monotonic():
    s = Seq()
    assert [s.next() for _ in range(3)] == [0, 1, 2]


def test_run_finished_status_validated():
    with pytest.raises(ValidationError):
        RunFinished.model_validate({"seq": 0, "status": "not-a-status"})


def test_schema_version_validated():
    with pytest.raises(ValidationError):
        RunFinished.model_validate({"seq": 0, "schema_version": 2, "status": "passed"})
