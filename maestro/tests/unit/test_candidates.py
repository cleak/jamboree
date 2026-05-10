from __future__ import annotations

import json
from typing import TYPE_CHECKING, Any, cast

from jam_maestro.candidates import (
    ProposeToolChangeRequest,
    RecordImprovementCandidateRequest,
    RecordTempyrUpdateCandidateRequest,
    propose_tool_change,
    record_improvement_candidate,
    record_tempyr_update_candidate,
)

if TYPE_CHECKING:
    from pathlib import Path


TRACE_ID = "01ARZ3NDEKTSV4RRFFQ69G5FAV"


def test_record_improvement_candidate_appends_jsonl(tmp_path: Path) -> None:
    result = record_improvement_candidate(
        RecordImprovementCandidateRequest(
            category="tooling",
            description="Add a narrower trace filter.",
            motivation="The current replay output is too broad.",
            originated_from_trace=TRACE_ID,
        ),
        root=tmp_path,
    )

    record = _read_one_jsonl(tmp_path / "improvement-candidates.jsonl")
    assert result.status == "queued"
    assert result.candidate_id == record["candidate_id"]
    assert record["kind"] == "improvement"
    assert record["payload"]["originated_from_trace"] == TRACE_ID


def test_propose_tool_change_appends_jsonl(tmp_path: Path) -> None:
    result = propose_tool_change(
        ProposeToolChangeRequest(
            spec={"name": "trace-summarize", "input": {"trace_id": "TraceId"}},
            rationale="Large traces need a compressed first pass.",
        ),
        root=tmp_path,
    )

    record = _read_one_jsonl(tmp_path / "tool-change-candidates.jsonl")
    assert result.candidate_id == record["candidate_id"]
    assert record["kind"] == "tool-change"
    assert record["payload"]["spec"]["name"] == "trace-summarize"


def test_record_tempyr_update_candidate_appends_jsonl(tmp_path: Path) -> None:
    result = record_tempyr_update_candidate(
        RecordTempyrUpdateCandidateRequest(
            candidate={"node_id": "comp-example", "status": "active"},
            reason="Observed implementation has landed.",
        ),
        root=tmp_path,
    )

    record = _read_one_jsonl(tmp_path / "tempyr-update-candidates.jsonl")
    assert result.candidate_id == record["candidate_id"]
    assert record["kind"] == "tempyr-update"
    assert record["payload"]["candidate"]["node_id"] == "comp-example"


def _read_one_jsonl(path: Path) -> dict[str, Any]:
    lines = path.read_text(encoding="utf-8").splitlines()
    assert len(lines) == 1
    value = json.loads(lines[0])
    assert isinstance(value, dict)
    return cast("dict[str, Any]", value)
