from __future__ import annotations

import json
from typing import TYPE_CHECKING

import pytest

from jam_maestro.journal_reader import ReadJournalRequest, read_journal

if TYPE_CHECKING:
    from pathlib import Path


TRACE_ID = "01ARZ3NDEKTSV4RRFFQ69G5FAV"
OTHER_TRACE_ID = "01BRZ3NDEKTSV4RRFFQ69G5FAA"


def test_read_journal_filters_by_trace_and_payload(tmp_path: Path) -> None:
    day = tmp_path / "2026-05-06"
    day.mkdir()
    path = day / "journal.picker.jsonl"
    _write_jsonl(
        path,
        {
            "event_type": "picker.spawned",
            "timestamp": "2026-05-06T01:00:00Z",
            "journal_seq": 1,
            "trace_id": TRACE_ID,
            "actor": "jam-svc-session",
            "payload": {"task_id": "task-1", "session_id": "codex-cli:abc"},
        },
        {
            "event_type": "picker.exited",
            "timestamp": "2026-05-06T01:05:00Z",
            "journal_seq": 2,
            "trace_id": OTHER_TRACE_ID,
            "actor": "jam-svc-session",
            "payload": {"task_id": "task-2", "session_id": "codex-cli:def"},
        },
    )

    result = read_journal(
        ReadJournalRequest(trace_id=TRACE_ID, task_id="task-1"),
        root=tmp_path,
    )

    assert len(result.entries) == 1
    assert result.entries[0]["event_type"] == "picker.spawned"
    assert result.entries[0]["_line_number"] == 1


def test_read_journal_applies_limit_to_newest_entries(tmp_path: Path) -> None:
    day = tmp_path / "2026-05-06"
    day.mkdir()
    _write_jsonl(
        day / "journal.task.jsonl",
        {
            "event_type": "task.requested",
            "timestamp": "2026-05-06T01:00:00Z",
            "journal_seq": 1,
            "trace_id": TRACE_ID,
            "actor": "human:caleb",
            "payload": {"task_id": "task-1"},
        },
        {
            "event_type": "task.completed",
            "timestamp": "2026-05-06T01:10:00Z",
            "journal_seq": 2,
            "trace_id": TRACE_ID,
            "actor": "jam-task-lifecycle",
            "payload": {"task_id": "task-1"},
        },
    )

    result = read_journal(ReadJournalRequest(trace_id=TRACE_ID, limit=1), root=tmp_path)

    assert [entry["event_type"] for entry in result.entries] == ["task.completed"]


def test_read_journal_fails_loudly_on_malformed_json(tmp_path: Path) -> None:
    day = tmp_path / "2026-05-06"
    day.mkdir()
    (day / "journal.bad.jsonl").write_text("{bad json}\n", encoding="utf-8")

    with pytest.raises(ValueError, match="malformed journal JSON"):
        read_journal(ReadJournalRequest(), root=tmp_path)


def _write_jsonl(path: Path, *entries: dict[str, object]) -> None:
    path.write_text(
        "".join(f"{json.dumps(entry, sort_keys=True)}\n" for entry in entries),
        encoding="utf-8",
    )
