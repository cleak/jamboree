"""Implementation of the local `record-learning` Maestro meta tool."""

from __future__ import annotations

import json
import os
import re
import tomllib
from datetime import UTC, date, datetime
from pathlib import Path
from typing import TYPE_CHECKING, cast

from pydantic import Field

from jam_maestro.models import StrictBaseModel, TraceId
from jam_maestro.paths import jam_home
from jam_maestro.tempyr_journal import (
    CliTempyrJournal,
    DecisionEntry,
    TempyrJournalClient,
)

if TYPE_CHECKING:
    from collections.abc import Mapping, Sequence

DEFAULT_SKILLS_ROOT = Path("/home/caleb/jamboree/skills")
_SLUG_RE = re.compile(r"[^a-z0-9]+")


class RecordLearningRequest(StrictBaseModel):
    """Inputs for `record-learning`."""

    scope: str = Field(min_length=1)
    evidence: str = Field(min_length=1)
    guidance: str = Field(min_length=1)
    confidence: float = Field(ge=0, le=1)
    originated_from_trace: TraceId
    counterexample: str | None = Field(default=None, min_length=1)
    title: str | None = Field(default=None, min_length=1, max_length=120)
    authored_by: str = Field(default="maestro", min_length=1)


class RecordLearningResult(StrictBaseModel):
    """Result of writing a skill note and paired Tempyr journal entry."""

    skill_path: str
    scope: str
    originated_from_trace: TraceId
    tempyr_logged: bool
    journal_flushed: bool = False
    journal_flush_error: str | None = None


async def record_learning(
    request: RecordLearningRequest,
    *,
    skills_root: Path | None = None,
    journal: TempyrJournalClient | None = None,
    agent: str | None = None,
    now: date | None = None,
) -> RecordLearningResult:
    """Write the skill note and emit the paired Tempyr decision entry."""
    active_root = skills_root or default_skills_root()
    active_now = now or datetime.now(UTC).date()
    skill_path = _next_available_path(_skill_path_for_scope(active_root, request.scope))
    rendered = _render_skill_note(request, active_now)
    skill_path.parent.mkdir(parents=True, exist_ok=True)
    skill_path.write_text(rendered, encoding="utf-8")

    active_agent = agent or f"maestro:record-learning:{active_now.isoformat()}"
    active_journal = journal or CliTempyrJournal()
    await active_journal.bootstrap(active_agent)
    await active_journal.log_decision(
        active_agent,
        _journal_entry(request, skill_path, active_root),
    )
    finalize_result = await active_journal.finalize(active_agent)

    return RecordLearningResult(
        skill_path=str(skill_path),
        scope=request.scope,
        originated_from_trace=request.originated_from_trace,
        tempyr_logged=True,
        journal_flushed=finalize_result.flushed,
        journal_flush_error=finalize_result.flush_error,
    )


def default_skills_root() -> Path:
    """Return the first configured skills folder, falling back to the monorepo skills dir."""
    explicit = os.environ.get("JAM_SKILLS_ROOT")
    if explicit:
        return Path(explicit)

    config = _skills_config_path()
    if config.exists():
        with config.open("rb") as handle:
            raw = cast("Mapping[str, object]", tomllib.load(handle))
        skills = raw.get("skills")
        if isinstance(skills, dict):
            skills_config = cast("Mapping[str, object]", skills)
            folders_obj = skills_config.get("folders")
            if isinstance(folders_obj, list):
                folders = cast("list[object]", folders_obj)
                for folder in folders:
                    if isinstance(folder, str):
                        return Path(folder)

    return DEFAULT_SKILLS_ROOT


def _skills_config_path() -> Path:
    explicit = os.environ.get("JAM_SKILLS_CONFIG")
    if explicit:
        return Path(explicit)
    return jam_home() / "config" / "skills.toml"


def _render_skill_note(request: RecordLearningRequest, today: date) -> str:
    title = request.title or _title_from_scope(request.scope)
    lines = [
        "---",
        f"date: {today.isoformat()}",
        f"scope: {_yaml_string(request.scope)}",
        f"confidence: {request.confidence:.2f}",
        f"authored-by: {_yaml_string(request.authored_by)}",
        f"originated-from-trace: {_yaml_string(request.originated_from_trace)}",
        *_yaml_block("evidence", request.evidence),
        *_yaml_block("guidance", request.guidance),
    ]
    if request.counterexample:
        lines.extend(_yaml_block("counterexample", request.counterexample))
    lines.extend(
        [
            "---",
            "",
            f"## {today.isoformat()} - {title}",
            "",
            "### Tempyr Journal",
            (
                "Paired decision entry is tagged "
                f"`trace:{request.originated_from_trace}` and `skill:{request.scope}`."
            ),
            "",
            "### Evidence",
            request.evidence.strip(),
            "",
            "### Guidance",
            request.guidance.strip(),
        ]
    )
    if request.counterexample:
        lines.extend(["", "### Counterexample", request.counterexample.strip()])
    lines.append("")
    return "\n".join(lines)


def _journal_entry(
    request: RecordLearningRequest,
    skill_path: Path,
    skills_root: Path,
) -> DecisionEntry:
    skill_ref = _relative_skill_ref(skill_path, skills_root)
    return DecisionEntry(
        summary=f"Recorded learning for {request.scope}"[:200],
        chosen=f"write skill note {skill_ref}",
        rationale=(
            "The Maestro observed a reusable pattern and persisted it as a scoped skill "
            "so future sessions can load the guidance directly."
        ),
        detail=(
            f"record-learning wrote {skill_ref} for scope {request.scope}. "
            f"Evidence: {request.evidence.strip()} Guidance: {request.guidance.strip()} "
            f"Originated from trace {request.originated_from_trace}."
        ),
        reversible=True,
        tags=[
            "tool:record-learning",
            f"skill:{request.scope}",
            f"trace:{request.originated_from_trace}",
            f"confidence:{request.confidence:.2f}",
        ],
        files=[skill_ref],
        refs=["task-record-learning-tool"],
    )


def _skill_path_for_scope(skills_root: Path, scope: str) -> Path:
    parts = [part for part in scope.split("/") if part]
    if parts and parts[0] == "blueberry":
        slug_source = "/".join(parts[1:]) or "learning"
        return skills_root / "projects" / "blueberry" / f"{_slugify(slug_source)}.md"
    return skills_root / "generated" / f"{_slugify(scope)}.md"


def _next_available_path(path: Path) -> Path:
    if not path.exists():
        return path
    for index in range(2, 1000):
        candidate = path.with_name(f"{path.stem}-{index}{path.suffix}")
        if not candidate.exists():
            return candidate
    message = f"could not allocate unique skill path for {path}"
    raise FileExistsError(message)


def _relative_skill_ref(skill_path: Path, skills_root: Path) -> str:
    try:
        return str(skill_path.relative_to(skills_root))
    except ValueError:
        return str(skill_path)


def _title_from_scope(scope: str) -> str:
    last = next((part for part in reversed(scope.split("/")) if part), scope)
    return last.replace("-", " ").replace("_", " ").strip().title() or "Recorded Learning"


def _slugify(value: str) -> str:
    lowered = value.lower()
    slug = _SLUG_RE.sub("-", lowered).strip("-")
    return slug[:80].strip("-") or "learning"


def _yaml_string(value: str) -> str:
    return json.dumps(value)


def _yaml_block(key: str, value: str) -> Sequence[str]:
    lines = [f"{key}: |-"]
    lines.extend(f"  {line}" if line else "  " for line in value.strip().splitlines())
    return lines
