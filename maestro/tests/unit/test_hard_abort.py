from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

from jam_maestro.hard_abort import (
    HardAbortDump,
    hard_abort_dump_path,
    read_hard_abort_dump,
    write_hard_abort_dump,
)
from jam_maestro.models import Message

if TYPE_CHECKING:
    from pathlib import Path

TRACE_ID = "01HXKJ00000000000000000000"


def test_hard_abort_dump_round_trips(tmp_path: Path) -> None:
    dump = HardAbortDump(
        session_id="maestro-session-2026-05-06",
        trace_id=TRACE_ID,
        reason="per-session-usd-exceeded-125pct",
        spent_usd=6.27,
        budget_usd=5.0,
        input_tokens_total=187432,
        output_tokens_total=41203,
        tool_calls_made=23,
        tool_calls_pending=1,
        task_in_flight="task-canyon-spline-refactor",
        last_world_snapshot={"readiness": {"status": "blocked"}},
        last_assistant_message="I need to check the CI status.",
        messages_in_session=[Message(role="assistant", content="I need to check CI.")],
    )

    path = write_hard_abort_dump(dump, root=tmp_path)
    loaded = read_hard_abort_dump(dump.session_id, root=tmp_path)

    assert path == tmp_path / "maestro-aborted-sessions" / f"{dump.session_id}.json"
    assert loaded == dump
    assert path.read_text(encoding="utf-8").endswith("\n")


def test_hard_abort_dump_path_rejects_traversal(tmp_path: Path) -> None:
    with pytest.raises(ValueError, match="session id"):
        hard_abort_dump_path("../bad", root=tmp_path)
