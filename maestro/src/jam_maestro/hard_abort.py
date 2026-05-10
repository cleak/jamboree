"""Hard-abort dump support for Maestro budget enforcement."""

from __future__ import annotations

import re
from datetime import UTC, datetime
from typing import TYPE_CHECKING, Literal

from pydantic import Field

from jam_maestro.models import Message, StrictBaseModel, TraceId
from jam_maestro.paths import jam_home

if TYPE_CHECKING:
    from pathlib import Path

SESSION_ID_PATTERN = re.compile(r"^[A-Za-z0-9_.:-]+$")


def _empty_snapshot() -> dict[str, object]:
    return {}


def _empty_messages() -> list[Message]:
    return []


class HardAbortDump(StrictBaseModel):
    """Durable state written when a Maestro session hard-aborts at 125% budget."""

    schema_version: Literal[1] = 1
    session_id: str = Field(min_length=1, pattern=SESSION_ID_PATTERN.pattern)
    trace_id: TraceId
    aborted_at: datetime = Field(default_factory=lambda: datetime.now(UTC))
    reason: Literal["per-session-usd-exceeded-125pct"]
    spent_usd: float = Field(ge=0)
    budget_usd: float = Field(gt=0)
    input_tokens_total: int = Field(ge=0)
    output_tokens_total: int = Field(ge=0)
    tool_calls_made: int = Field(ge=0)
    tool_calls_pending: int = Field(ge=0)
    task_in_flight: str = Field(min_length=1)
    last_world_snapshot: dict[str, object] = Field(default_factory=_empty_snapshot)
    last_assistant_message: str | None = None
    messages_in_session: list[Message] = Field(default_factory=_empty_messages)


def hard_abort_dump_path(session_id: str, *, root: Path | None = None) -> Path:
    """Return the canonical hard-abort dump path for a session."""
    _validate_session_id(session_id)
    active_root = root or jam_home()
    return active_root / "maestro-aborted-sessions" / f"{session_id}.json"


def write_hard_abort_dump(dump: HardAbortDump, *, root: Path | None = None) -> Path:
    """Atomically write a hard-abort dump and return its path."""
    path = hard_abort_dump_path(dump.session_id, root=root)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(f".{path.name}.tmp")
    tmp.write_text(dump.model_dump_json(indent=2) + "\n", encoding="utf-8")
    tmp.replace(path)
    return path


def read_hard_abort_dump(session_id: str, *, root: Path | None = None) -> HardAbortDump:
    """Read and validate a hard-abort dump."""
    path = hard_abort_dump_path(session_id, root=root)
    return HardAbortDump.model_validate_json(path.read_text(encoding="utf-8"))


def _validate_session_id(session_id: str) -> None:
    if session_id.startswith(".") or not SESSION_ID_PATTERN.fullmatch(session_id):
        message = "session id may only contain ASCII letters, numbers, `-`, `_`, `.`, and `:`"
        raise ValueError(message)
