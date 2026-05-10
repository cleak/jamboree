"""Tempyr journal wrapper for Maestro sessions."""

from __future__ import annotations

import asyncio
import os
import tomllib
from pathlib import Path
from typing import Literal, Protocol, cast

from pydantic import Field, field_validator

from jam_maestro.models import StrictBaseModel
from jam_maestro.paths import jam_home


class TempyrJournalError(RuntimeError):
    """Raised when Tempyr journal operations fail."""


type Confidence = Literal["low", "medium", "high"]
type Severity = Literal["info", "warn", "high", "blocker"]
type AssumptionPolarity = Literal["positive", "negative", "unknown"]


class BaseJournalEntry(StrictBaseModel):
    """Common fields accepted by `tempyr journal log` entries."""

    summary: str = Field(min_length=20, max_length=200)
    tags: list[str] = Field(default_factory=list)
    files: list[str] = Field(default_factory=list)
    refs: list[str] = Field(default_factory=list)
    provisional: bool = False
    confidence: Confidence | None = None
    severity: Severity | None = None


class PlanEntry(BaseJournalEntry):
    """Fields for a Tempyr `plan` entry."""

    kind: Literal["plan"] = "plan"
    detail: str | None = Field(default=None, min_length=1)


class FindingEntry(BaseJournalEntry):
    """Fields for a Tempyr `finding` entry."""

    kind: Literal["finding"] = "finding"
    detail: str | None = Field(default=None, min_length=1)


class AssumptionEntry(BaseJournalEntry):
    """Fields required for a Tempyr `assumption` entry."""

    kind: Literal["assumption"] = "assumption"
    detail: str | None = Field(default=None, min_length=1)
    polarity: AssumptionPolarity


class QuestionEntry(BaseJournalEntry):
    """Fields for a Tempyr `question` entry."""

    kind: Literal["question"] = "question"
    detail: str | None = Field(default=None, min_length=1)


class DecisionEntry(BaseJournalEntry):
    """Fields required for a Tempyr `decision` entry."""

    kind: Literal["decision"] = "decision"
    summary: str = Field(min_length=20, max_length=200)
    chosen: str = Field(min_length=1)
    rationale: str = Field(min_length=1)
    detail: str | None = Field(min_length=50)
    reversible: bool = True
    alternatives: list[str] = Field(default_factory=list)

    @field_validator("detail")
    @classmethod
    def _require_detail(cls, value: str | None) -> str:
        if value is None:
            message = "decision detail is required"
            raise ValueError(message)
        return value


class DeadEndEntry(BaseJournalEntry):
    """Fields required for a Tempyr `dead_end` entry."""

    kind: Literal["dead_end"] = "dead_end"
    summary: str = Field(min_length=20, max_length=200)
    detail: str | None = Field(min_length=50)
    approach: str = Field(min_length=1)
    failure_mode: str = Field(min_length=1)
    next_to_try: str | None = Field(default=None, min_length=1)

    @field_validator("detail")
    @classmethod
    def _require_detail(cls, value: str | None) -> str:
        if value is None:
            message = "dead_end detail is required"
            raise ValueError(message)
        return value


class RiskEntry(BaseJournalEntry):
    """Fields for a Tempyr `risk` entry."""

    kind: Literal["risk"] = "risk"
    detail: str | None = Field(default=None, min_length=1)


class OutcomeEntry(BaseJournalEntry):
    """Fields for a final Tempyr `outcome` entry."""

    kind: Literal["outcome"] = "outcome"
    summary: str = Field(min_length=20, max_length=200)
    detail: str = Field(min_length=1)
    passed: bool | None = None
    build_ok: bool | None = None
    commit_sha: str | None = Field(default=None, min_length=1)
    final: bool = True


type JournalEntry = (
    PlanEntry
    | FindingEntry
    | AssumptionEntry
    | QuestionEntry
    | DecisionEntry
    | DeadEndEntry
    | RiskEntry
    | OutcomeEntry
)


class JournalFinalizeResult(StrictBaseModel):
    """Result of finalizing and publishing a Tempyr journal session."""

    flushed: bool = False
    flush_error: str | None = None


class TempyrJournalClient(Protocol):
    """Journal lifecycle used by the Maestro session loop."""

    async def bootstrap(self, agent: str) -> None:
        """Ensure the journal exists before logging."""
        ...

    async def log_decision(self, agent: str, entry: DecisionEntry) -> None:
        """Append a decision entry."""
        ...

    async def log_outcome(self, agent: str, entry: OutcomeEntry) -> None:
        """Append a final outcome entry."""
        ...

    async def log_entry(self, agent: str, entry: JournalEntry) -> None:
        """Append any typed Tempyr journal entry."""
        ...

    async def finalize(self, agent: str) -> JournalFinalizeResult:
        """Finalize the active session for `agent`."""
        ...


class NullTempyrJournal:
    """Test-only journal used when unit tests do not touch Tempyr."""

    async def bootstrap(self, agent: str) -> None:
        _ = agent

    async def log_decision(self, agent: str, entry: DecisionEntry) -> None:
        _ = (agent, entry)

    async def log_outcome(self, agent: str, entry: OutcomeEntry) -> None:
        _ = (agent, entry)

    async def log_entry(self, agent: str, entry: JournalEntry) -> None:
        _ = (agent, entry)

    async def finalize(self, agent: str) -> JournalFinalizeResult:
        _ = agent
        return JournalFinalizeResult()


class CliTempyrJournal:
    """Call `tempyr journal` from the canonical worktree."""

    def __init__(self, *, worktree: Path | None = None, tempyr_bin: str = "tempyr") -> None:
        self._worktree = worktree or default_tempyr_worktree()
        self._tempyr_bin = tempyr_bin

    async def bootstrap(self, agent: str) -> None:
        _ = agent
        await self._run("journal", "bootstrap", "--quiet")

    async def log_decision(self, agent: str, entry: DecisionEntry) -> None:
        await self.log_entry(agent, entry)

    async def log_outcome(self, agent: str, entry: OutcomeEntry) -> None:
        await self.log_entry(agent, entry)

    async def log_entry(self, agent: str, entry: JournalEntry) -> None:
        """Append any of Tempyr's eight typed journal entry kinds."""
        await self._run(*_journal_log_args(agent, entry))

    async def finalize(self, agent: str) -> JournalFinalizeResult:
        await self._run("journal", "finalize", "--agent", agent, "--quiet")
        try:
            await self._run("journal", "flush")
        except TempyrJournalError as exc:
            return JournalFinalizeResult(flush_error=str(exc))
        return JournalFinalizeResult(flushed=True)

    async def _run(self, *args: str) -> None:
        if not self._worktree.is_dir():
            message = f"Tempyr worktree does not exist: {self._worktree}"
            raise TempyrJournalError(message)

        process = await asyncio.create_subprocess_exec(
            self._tempyr_bin,
            *args,
            cwd=self._worktree,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        stdout, stderr = await process.communicate()
        if process.returncode != 0:
            detail = stderr.decode().strip() or stdout.decode().strip()
            message = f"tempyr {' '.join(args)} failed: {detail}"
            raise TempyrJournalError(message)


def _journal_log_args(agent: str, entry: JournalEntry) -> list[str]:
    args = ["journal", "log", "--agent", agent, entry.kind, entry.summary]
    _append_common_entry_args(args, entry)
    _append_kind_specific_args(args, entry)
    return args


def _append_kind_specific_args(args: list[str], entry: JournalEntry) -> None:
    if isinstance(entry, DecisionEntry):
        _append_decision_args(args, entry)
    elif isinstance(entry, DeadEndEntry):
        _append_dead_end_args(args, entry)
    elif isinstance(entry, AssumptionEntry):
        args.extend(("--polarity", entry.polarity))
    elif isinstance(entry, OutcomeEntry):
        _append_outcome_args(args, entry)


def _append_decision_args(args: list[str], entry: DecisionEntry) -> None:
    args.extend(
        (
            "--chosen",
            entry.chosen,
            "--rationale",
            entry.rationale,
            "--reversible",
            "true" if entry.reversible else "false",
        )
    )
    for alternative in entry.alternatives:
        args.extend(("--alternative", alternative))


def _append_dead_end_args(args: list[str], entry: DeadEndEntry) -> None:
    args.extend(("--approach", entry.approach, "--failure-mode", entry.failure_mode))
    if entry.next_to_try is not None:
        args.extend(("--next-to-try", entry.next_to_try))


def _append_outcome_args(args: list[str], entry: OutcomeEntry) -> None:
    if entry.passed is not None:
        args.extend(("--passed", "true" if entry.passed else "false"))
    if entry.build_ok is not None:
        args.extend(("--build-ok", "true" if entry.build_ok else "false"))
    if entry.commit_sha is not None:
        args.extend(("--commit-sha", entry.commit_sha))
    if entry.final:
        args.append("--final")


def _append_common_entry_args(args: list[str], entry: JournalEntry) -> None:
    if entry.detail is not None:
        args.extend(("--detail", entry.detail))
    for tag in entry.tags:
        args.extend(("--tag", tag))
    for file in entry.files:
        args.extend(("--file", file))
    for ref in entry.refs:
        args.extend(("--ref", ref))
    if entry.provisional:
        args.append("--provisional")
    if entry.confidence is not None:
        args.extend(("--confidence", entry.confidence))
    if entry.severity is not None:
        args.extend(("--severity", entry.severity))


def default_tempyr_worktree() -> Path:
    """Return the canonical Tempyr worktree for the single Blueberry instance."""
    explicit = os.environ.get("JAM_TEMPYR_WORKTREE")
    if explicit:
        return Path(explicit)

    project_config = _project_config_path()
    if project_config.exists():
        with project_config.open("rb") as handle:
            raw = tomllib.load(handle)
        config = cast("dict[str, object]", raw)
        value = config.get("canonical-worktree")
        if isinstance(value, str):
            return Path(value)

    return Path("/home/caleb/blueberry-jam")


def _project_config_path() -> Path:
    explicit = os.environ.get("JAM_PROJECT_CONFIG")
    if explicit:
        return Path(explicit)
    return jam_home() / "config" / "projects" / "blueberry.toml"
