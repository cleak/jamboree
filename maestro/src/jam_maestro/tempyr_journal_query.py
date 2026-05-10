"""Read-only Tempyr journal query wrappers for the Maestro."""

from __future__ import annotations

import json
import subprocess
from pathlib import Path
from typing import Literal, Protocol, cast

from pydantic import Field

from jam_maestro.models import StrictBaseModel


class TempyrJournalSearchRequest(StrictBaseModel):
    """Inputs for `tempyr-journal-search`."""

    query: str = Field(min_length=1, max_length=500)
    kind: list[str] = Field(default_factory=list, max_length=8)
    agent: str | None = Field(default=None, min_length=1, max_length=200)
    since_days: int | None = Field(default=None, ge=1, le=3650)
    limit: int = Field(default=10, ge=1, le=50)
    token_budget: int | None = Field(default=None, ge=256, le=20000)


class TempyrJournalRangeRequest(StrictBaseModel):
    """Inputs for `tempyr-journal-range`."""

    rev_range: str = Field(min_length=1, max_length=300)
    kind: list[str] = Field(default_factory=list, max_length=8)
    limit: int = Field(default=50, ge=1, le=100)
    token_budget: int | None = Field(default=None, ge=256, le=20000)


class TempyrJournalBlameRequest(StrictBaseModel):
    """Inputs for `tempyr-journal-blame`."""

    file_path: str = Field(min_length=1, max_length=500)
    kind: list[str] = Field(default_factory=list, max_length=8)
    limit: int = Field(default=50, ge=1, le=100)
    token_budget: int | None = Field(default=None, ge=256, le=20000)


class TempyrJournalHit(StrictBaseModel):
    """One Tempyr journal hit."""

    entry: dict[str, object]
    score: float | None = None


class TempyrJournalQueryResult(StrictBaseModel):
    """Normalized Tempyr journal query result."""

    mode: Literal["search", "range", "blame"]
    count: int
    hits: list[TempyrJournalHit]
    query: str | None = None
    rev_range: str | None = None
    file_path: str | None = None


class TempyrJournalRunner(Protocol):
    """Subprocess-compatible command runner."""

    def __call__(
        self,
        args: list[str],
        *,
        cwd: Path,
        text: bool,
        capture_output: bool,
        check: bool,
    ) -> subprocess.CompletedProcess[str]:
        """Run a command and return the completed process."""
        ...


def tempyr_journal_search(
    request: TempyrJournalSearchRequest,
    *,
    cwd: Path | None = None,
    runner: TempyrJournalRunner = subprocess.run,
) -> TempyrJournalQueryResult:
    """Run a bounded local Tempyr journal search."""
    args = _base_args("search", request.kind, request.limit, request.token_budget)
    if request.since_days is not None:
        args.extend(["--since-days", str(request.since_days)])
    args.append(request.query)
    result = _run_json(args, cwd=cwd, runner=runner)
    hits = _parse_hits(result)
    if request.agent is not None:
        hits = [hit for hit in hits if hit.entry.get("agent") == request.agent]
    return TempyrJournalQueryResult(
        mode="search",
        query=request.query,
        count=len(hits),
        hits=hits,
    )


def tempyr_journal_range(
    request: TempyrJournalRangeRequest,
    *,
    cwd: Path | None = None,
    runner: TempyrJournalRunner = subprocess.run,
) -> TempyrJournalQueryResult:
    """Run a bounded local Tempyr journal range query."""
    args = _base_args("range", request.kind, request.limit, request.token_budget)
    args.append(request.rev_range)
    result = _run_json(args, cwd=cwd, runner=runner)
    hits = _parse_hits(result)
    return TempyrJournalQueryResult(
        mode="range",
        rev_range=request.rev_range,
        count=len(hits),
        hits=hits,
    )


def tempyr_journal_blame(
    request: TempyrJournalBlameRequest,
    *,
    cwd: Path | None = None,
    runner: TempyrJournalRunner = subprocess.run,
) -> TempyrJournalQueryResult:
    """Run a bounded local Tempyr journal blame query."""
    args = _base_args("blame", request.kind, request.limit, request.token_budget)
    args.append(request.file_path)
    result = _run_json(args, cwd=cwd, runner=runner)
    hits = _parse_hits(result)
    return TempyrJournalQueryResult(
        mode="blame",
        file_path=request.file_path,
        count=len(hits),
        hits=hits,
    )


def _base_args(
    command: Literal["search", "range", "blame"],
    kinds: list[str],
    limit: int,
    token_budget: int | None,
) -> list[str]:
    args = ["tempyr", "journal", command, "--json", "--limit", str(limit)]
    for kind in kinds:
        args.extend(["--kind", kind])
    if token_budget is not None:
        args.extend(["--token-budget", str(token_budget)])
    return args


def _run_json(
    args: list[str],
    *,
    cwd: Path | None,
    runner: TempyrJournalRunner,
) -> object:
    completed = runner(
        args,
        cwd=cwd or Path.cwd(),
        text=True,
        capture_output=True,
        check=False,
    )
    if completed.returncode != 0:
        detail = completed.stderr.strip() or completed.stdout.strip()
        message = f"tempyr journal query failed: {detail}"
        raise RuntimeError(message)
    try:
        return json.loads(completed.stdout)
    except json.JSONDecodeError as exc:
        message = f"tempyr journal query returned malformed JSON: {exc}"
        raise ValueError(message) from exc


def _parse_hits(decoded: object) -> list[TempyrJournalHit]:
    if not isinstance(decoded, dict):
        message = "tempyr journal query JSON result was not an object"
        raise TypeError(message)
    payload = cast("dict[str, object]", decoded)
    raw_hits = payload.get("hits")
    if not isinstance(raw_hits, list):
        message = "tempyr journal query JSON result missing hits[]"
        raise TypeError(message)
    hits = cast("list[object]", raw_hits)
    return [_parse_hit(hit) for hit in hits]


def _parse_hit(raw: object) -> TempyrJournalHit:
    if not isinstance(raw, dict):
        message = "tempyr journal hit was not an object"
        raise TypeError(message)
    hit = cast("dict[str, object]", raw)
    entry = hit.get("entry")
    if not isinstance(entry, dict):
        message = "tempyr journal hit missing entry object"
        raise TypeError(message)
    return TempyrJournalHit(
        entry=cast("dict[str, object]", entry),
        score=_optional_float(hit.get("score")),
    )


def _optional_float(value: object) -> float | None:
    if isinstance(value, bool):
        return None
    if isinstance(value, int | float):
        return float(value)
    return None
