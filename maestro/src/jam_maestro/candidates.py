"""Local human-review candidate queues for Maestro meta-tools."""

from __future__ import annotations

import json
from datetime import UTC, datetime
from typing import TYPE_CHECKING, Any, Literal
from uuid import uuid4

from pydantic import Field

from jam_maestro.models import StrictBaseModel, TraceId
from jam_maestro.paths import jam_home

if TYPE_CHECKING:
    from pathlib import Path


class RecordImprovementCandidateRequest(StrictBaseModel):
    """Inputs for `record-improvement-candidate`."""

    category: str = Field(min_length=1, max_length=100)
    description: str = Field(min_length=1, max_length=4000)
    motivation: str = Field(min_length=1, max_length=4000)
    originated_from_trace: TraceId | None = None


class ProposeToolChangeRequest(StrictBaseModel):
    """Inputs for `propose-tool-change`."""

    spec: dict[str, Any] = Field(min_length=1)
    rationale: str = Field(min_length=1, max_length=4000)
    originated_from_trace: TraceId | None = None


class RecordTempyrUpdateCandidateRequest(StrictBaseModel):
    """Inputs for `record-tempyr-update-candidate`."""

    candidate: dict[str, Any] = Field(min_length=1)
    reason: str = Field(min_length=1, max_length=4000)
    originated_from_trace: TraceId | None = None


class CandidateRecord(StrictBaseModel):
    """One queued human-review candidate."""

    candidate_id: str
    kind: Literal["improvement", "tool-change", "tempyr-update"]
    created_at: datetime
    payload: dict[str, Any]


class CandidateResult(StrictBaseModel):
    """Result returned after a candidate is appended."""

    candidate_id: str
    path: str
    status: Literal["queued"] = "queued"


def record_improvement_candidate(
    request: RecordImprovementCandidateRequest,
    *,
    root: Path | None = None,
) -> CandidateResult:
    """Append an improvement candidate for human review."""
    return _append_candidate(
        "improvement",
        request.model_dump(mode="json", exclude_none=True),
        _candidate_path("improvement-candidates.jsonl", root=root),
    )


def propose_tool_change(
    request: ProposeToolChangeRequest,
    *,
    root: Path | None = None,
) -> CandidateResult:
    """Append a proposed tool-surface change for human review."""
    return _append_candidate(
        "tool-change",
        request.model_dump(mode="json", exclude_none=True),
        _candidate_path("tool-change-candidates.jsonl", root=root),
    )


def record_tempyr_update_candidate(
    request: RecordTempyrUpdateCandidateRequest,
    *,
    root: Path | None = None,
) -> CandidateResult:
    """Append a Tempyr update candidate for human review."""
    return _append_candidate(
        "tempyr-update",
        request.model_dump(mode="json", exclude_none=True),
        _candidate_path("tempyr-update-candidates.jsonl", root=root),
    )


def _candidate_path(filename: str, *, root: Path | None) -> Path:
    return (root or jam_home()) / filename


def _append_candidate(
    kind: Literal["improvement", "tool-change", "tempyr-update"],
    payload: dict[str, Any],
    path: Path,
) -> CandidateResult:
    now = datetime.now(tz=UTC)
    record = CandidateRecord(
        candidate_id=f"{kind}:{now.strftime('%Y%m%dT%H%M%SZ')}:{uuid4().hex[:8]}",
        kind=kind,
        created_at=now,
        payload=payload,
    )
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(record.model_dump(mode="json"), sort_keys=True))
        handle.write("\n")
    return CandidateResult(candidate_id=record.candidate_id, path=str(path))
