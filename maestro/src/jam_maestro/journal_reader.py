"""Read-only orchestrator journal queries for the Maestro."""

from __future__ import annotations

import json
from typing import TYPE_CHECKING, Any, cast

from pydantic import Field

from jam_maestro.models import StrictBaseModel, TraceId
from jam_maestro.paths import jam_home

if TYPE_CHECKING:
    from pathlib import Path


class ReadJournalRequest(StrictBaseModel):
    """Inputs for `read-journal`."""

    trace_id: TraceId | None = None
    event_type: str | None = Field(default=None, min_length=1)
    task_id: str | None = Field(default=None, min_length=1)
    session_id: str | None = Field(default=None, min_length=1)
    pr_ref: str | None = Field(default=None, min_length=1)
    limit: int = Field(default=50, ge=1, le=500)


class ReadJournalResult(StrictBaseModel):
    """Filtered journal entries."""

    entries: list[dict[str, Any]]


def read_journal(
    request: ReadJournalRequest,
    *,
    root: Path | None = None,
) -> ReadJournalResult:
    """Read matching entries from rotated orchestrator JSONL journals."""
    journal_root = root or jam_home() / "journal"
    if not journal_root.exists():
        return ReadJournalResult(entries=[])

    entries: list[dict[str, Any]] = []
    for path in sorted(journal_root.glob("*/journal.*.jsonl")):
        entries.extend(_read_matching_path(path, request))
    entries.sort(key=_entry_sort_key)
    return ReadJournalResult(entries=entries[-request.limit :])


def _read_matching_path(path: Path, request: ReadJournalRequest) -> list[dict[str, Any]]:
    matches: list[dict[str, Any]] = []
    with path.open(encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, start=1):
            if not line.strip():
                continue
            try:
                raw = json.loads(line)
            except json.JSONDecodeError as exc:
                msg = f"{path}:{line_number}: malformed journal JSON: {exc}"
                raise ValueError(msg) from exc
            if not isinstance(raw, dict):
                msg = f"{path}:{line_number}: journal entry is not an object"
                raise TypeError(msg)
            entry = dict(cast("dict[str, Any]", raw))
            entry["_source_path"] = str(path)
            entry["_line_number"] = line_number
            if _matches(entry, request):
                matches.append(entry)
    return matches


def _matches(entry: dict[str, Any], request: ReadJournalRequest) -> bool:
    payload = entry.get("payload")
    payload_obj = cast("dict[str, Any]", payload) if isinstance(payload, dict) else {}
    return all(
        [
            request.trace_id is None or entry.get("trace_id") == request.trace_id,
            request.event_type is None or entry.get("event_type") == request.event_type,
            request.task_id is None or payload_obj.get("task_id") == request.task_id,
            request.session_id is None or payload_obj.get("session_id") == request.session_id,
            request.pr_ref is None or payload_obj.get("pr_ref") == request.pr_ref,
        ]
    )


def _entry_sort_key(entry: dict[str, Any]) -> tuple[str, int, str, int]:
    timestamp = entry.get("timestamp")
    journal_seq = entry.get("journal_seq")
    source_path = entry.get("_source_path")
    line_number = entry.get("_line_number")
    return (
        timestamp if isinstance(timestamp, str) else "",
        journal_seq if isinstance(journal_seq, int) else 0,
        source_path if isinstance(source_path, str) else "",
        line_number if isinstance(line_number, int) else 0,
    )
