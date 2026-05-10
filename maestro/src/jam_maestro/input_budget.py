"""Session-start input budget assembly for the Maestro."""

from __future__ import annotations

import json
import os
import tomllib
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING, cast

from pydantic import Field

from jam_maestro.models import StrictBaseModel
from jam_maestro.paths import jam_home

if TYPE_CHECKING:
    from jam_maestro.skills import LoadedSkill
    from jam_maestro.wake import TaskWake

BYTES_PER_TOKEN_ESTIMATE = 4
DEFAULT_PER_SESSION_INPUT_TOKENS = 200_000
DEFAULT_SKILL_FILES_MAX_BYTES = 80_000
DEFAULT_JOURNAL_REPLAY_MAX_EVENTS = 100
DEFAULT_WORLD_SNAPSHOT_MAX_BYTES = 40_000


class InputBudgetConfig(StrictBaseModel):
    """Config-backed caps for session-start context (§4.1.3)."""

    per_session_input_tokens: int = Field(default=DEFAULT_PER_SESSION_INPUT_TOKENS, gt=0)
    skill_files_max_bytes: int = Field(default=DEFAULT_SKILL_FILES_MAX_BYTES, ge=0)
    journal_replay_max_events: int = Field(default=DEFAULT_JOURNAL_REPLAY_MAX_EVENTS, ge=0)
    world_snapshot_max_bytes: int = Field(default=DEFAULT_WORLD_SNAPSHOT_MAX_BYTES, gt=0)

    @property
    def per_session_input_bytes(self) -> int:
        """Approximate byte cap from token budget using the local 4 bytes/token rule."""
        return self.per_session_input_tokens * BYTES_PER_TOKEN_ESTIMATE


class BudgetedSkill(StrictBaseModel):
    """One skill after input-budget truncation."""

    path: str
    content: str = ""
    original_bytes: int = Field(ge=0)
    included_bytes: int = Field(ge=0)
    truncated: bool = False


class InputBudgetReport(StrictBaseModel):
    """Machine-readable budget result for logging and tests."""

    input_bytes_total: int = Field(ge=0)
    wake_bytes: int = Field(ge=0)
    world_snapshot_bytes: int = Field(ge=0)
    world_snapshot_truncated: bool = False
    skill_bytes: int = Field(ge=0)
    skills_included: int = Field(ge=0)
    skills_truncated: int = Field(ge=0)
    skills_dropped: int = Field(ge=0)
    journal_events_included: int = Field(ge=0)
    journal_events_dropped: int = Field(ge=0)
    warnings: list[str] = Field(default_factory=list)


class SessionInputBundle(StrictBaseModel):
    """Budgeted session-start inputs in priority order."""

    wake_context: str
    world_snapshot: str
    skills: list[BudgetedSkill]
    journal_events: list[str] = Field(default_factory=list)
    report: InputBudgetReport


def load_input_budget_config(config_path: Path | None = None) -> InputBudgetConfig:
    """Load `[budget]` and `[input-budget]` from `maestro.toml`, defaulting when absent."""
    path = config_path or _default_maestro_config_path()
    if not path.exists():
        return InputBudgetConfig()

    with path.open("rb") as handle:
        raw = cast("dict[str, object]", tomllib.load(handle))
    budget = _mapping(raw.get("budget"))
    input_budget = _mapping(raw.get("input-budget"))
    return InputBudgetConfig(
        per_session_input_tokens=_int_or_default(
            budget.get("per-session-input-tokens"),
            DEFAULT_PER_SESSION_INPUT_TOKENS,
        ),
        skill_files_max_bytes=_int_or_default(
            input_budget.get("skill-files-max-bytes"),
            DEFAULT_SKILL_FILES_MAX_BYTES,
        ),
        journal_replay_max_events=_int_or_default(
            input_budget.get("journal-replay-max-events"),
            DEFAULT_JOURNAL_REPLAY_MAX_EVENTS,
        ),
        world_snapshot_max_bytes=_int_or_default(
            input_budget.get("world-snapshot-max-bytes"),
            DEFAULT_WORLD_SNAPSHOT_MAX_BYTES,
        ),
    )


def assemble_session_input(
    *,
    wake: TaskWake,
    world_snapshot: Mapping[str, object],
    skills: Sequence[LoadedSkill],
    journal_events: Sequence[Mapping[str, object]] = (),
    config: InputBudgetConfig | None = None,
) -> SessionInputBundle:
    """Assemble wake, snapshot, skills, and journal events within explicit caps."""
    active_config = config or InputBudgetConfig()
    wake_context = _json_text(wake.model_dump(mode="json"))
    world_snapshot_text, world_snapshot_truncated = _truncate_text(
        _json_text(world_snapshot),
        active_config.world_snapshot_max_bytes,
    )
    budgeted_skills = _budget_skills(skills, active_config.skill_files_max_bytes)
    journal_texts = [_json_text(event) for event in journal_events]
    included_journal = journal_texts[: active_config.journal_replay_max_events]

    bundle = SessionInputBundle(
        wake_context=wake_context,
        world_snapshot=world_snapshot_text,
        skills=budgeted_skills,
        journal_events=included_journal,
        report=_report(
            _ReportParts(
                wake_context=wake_context,
                world_snapshot=world_snapshot_text,
                world_snapshot_truncated=world_snapshot_truncated,
                skills=budgeted_skills,
                original_skill_count=len(skills),
                journal_events=included_journal,
                original_journal_count=len(journal_events),
                config=active_config,
            )
        ),
    )
    return _fit_total_budget(bundle, active_config)


def _default_maestro_config_path() -> Path:
    explicit = os.environ.get("JAM_MAESTRO_CONFIG")
    if explicit:
        return Path(explicit)
    return jam_home() / "config" / "maestro.toml"


def _mapping(value: object) -> Mapping[str, object]:
    if isinstance(value, Mapping):
        return cast("Mapping[str, object]", value)
    return {}


def _int_or_default(value: object, default: int) -> int:
    return value if isinstance(value, int) and not isinstance(value, bool) else default


def _json_text(value: object) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def _budget_skills(skills: Sequence[LoadedSkill], max_bytes: int) -> list[BudgetedSkill]:
    remaining = max_bytes
    budgeted: list[BudgetedSkill] = []
    for skill in skills:
        original_bytes = _byte_len(skill.content)
        if remaining <= 0:
            continue
        content, truncated = _truncate_text(skill.content, remaining)
        included_bytes = _byte_len(content)
        if included_bytes == 0:
            continue
        budgeted.append(
            BudgetedSkill(
                path=skill.path,
                content=content,
                original_bytes=original_bytes,
                included_bytes=included_bytes,
                truncated=truncated,
            )
        )
        remaining -= included_bytes
    return budgeted


def _fit_total_budget(bundle: SessionInputBundle, config: InputBudgetConfig) -> SessionInputBundle:
    if bundle.report.input_bytes_total <= config.per_session_input_bytes:
        return bundle

    skills = bundle.skills
    while (
        skills
        and _bundle_bytes(
            bundle.wake_context,
            bundle.world_snapshot,
            skills,
            bundle.journal_events,
        )
        > config.per_session_input_bytes
    ):
        skills = skills[:-1]

    journal_events = bundle.journal_events
    while (
        journal_events
        and _bundle_bytes(
            bundle.wake_context,
            bundle.world_snapshot,
            skills,
            journal_events,
        )
        > config.per_session_input_bytes
    ):
        journal_events = journal_events[:-1]

    warnings = list(bundle.report.warnings)
    current_bytes = _bundle_bytes(
        bundle.wake_context,
        bundle.world_snapshot,
        skills,
        journal_events,
    )
    if current_bytes > config.per_session_input_bytes:
        warnings.append("wake_context_and_world_snapshot_exceed_input_budget")

    return SessionInputBundle(
        wake_context=bundle.wake_context,
        world_snapshot=bundle.world_snapshot,
        skills=skills,
        journal_events=journal_events,
        report=_report(
            _ReportParts(
                wake_context=bundle.wake_context,
                world_snapshot=bundle.world_snapshot,
                world_snapshot_truncated=bundle.report.world_snapshot_truncated,
                skills=skills,
                original_skill_count=bundle.report.skills_included + bundle.report.skills_dropped,
                journal_events=journal_events,
                original_journal_count=bundle.report.journal_events_included
                + bundle.report.journal_events_dropped,
                config=config,
                extra_warnings=warnings,
            )
        ),
    )


@dataclass(frozen=True)
class _ReportParts:
    wake_context: str
    world_snapshot: str
    world_snapshot_truncated: bool
    skills: Sequence[BudgetedSkill]
    original_skill_count: int
    journal_events: Sequence[str]
    original_journal_count: int
    config: InputBudgetConfig
    extra_warnings: Sequence[str] = ()


def _report(parts: _ReportParts) -> InputBudgetReport:
    skill_bytes = sum(skill.included_bytes for skill in parts.skills)
    input_bytes_total = _bundle_bytes(
        parts.wake_context,
        parts.world_snapshot,
        parts.skills,
        parts.journal_events,
    )
    skills_truncated = sum(1 for skill in parts.skills if skill.truncated)
    skills_dropped = max(0, parts.original_skill_count - len(parts.skills))
    journal_events_dropped = max(0, parts.original_journal_count - len(parts.journal_events))
    warnings = list(parts.extra_warnings)
    if parts.world_snapshot_truncated:
        warnings.append("world_snapshot_truncated")
    if skill_bytes >= parts.config.skill_files_max_bytes and skills_dropped:
        warnings.append("skill_budget_exhausted")
    if journal_events_dropped:
        warnings.append("journal_replay_limited")
    return InputBudgetReport(
        input_bytes_total=input_bytes_total,
        wake_bytes=_byte_len(parts.wake_context),
        world_snapshot_bytes=_byte_len(parts.world_snapshot),
        world_snapshot_truncated=parts.world_snapshot_truncated,
        skill_bytes=skill_bytes,
        skills_included=len(parts.skills),
        skills_truncated=skills_truncated,
        skills_dropped=skills_dropped,
        journal_events_included=len(parts.journal_events),
        journal_events_dropped=journal_events_dropped,
        warnings=warnings,
    )


def _bundle_bytes(
    wake_context: str,
    world_snapshot: str,
    skills: Sequence[BudgetedSkill],
    journal_events: Sequence[str],
) -> int:
    return (
        _byte_len(wake_context)
        + _byte_len(world_snapshot)
        + sum(skill.included_bytes for skill in skills)
        + sum(_byte_len(event) for event in journal_events)
    )


def _truncate_text(value: str, max_bytes: int) -> tuple[str, bool]:
    encoded = value.encode("utf-8")
    if len(encoded) <= max_bytes:
        return value, False
    if max_bytes <= 0:
        return "", True
    return encoded[:max_bytes].decode("utf-8", errors="ignore"), True


def _byte_len(value: str) -> int:
    return len(value.encode("utf-8"))
