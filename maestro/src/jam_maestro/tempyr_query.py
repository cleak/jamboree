"""Read-only Tempyr graph queries for the Maestro."""

from __future__ import annotations

import json
import subprocess
from pathlib import Path
from typing import Protocol, cast

from pydantic import Field

from jam_maestro.models import StrictBaseModel


class QueryTempyrRequest(StrictBaseModel):
    """Inputs for `query-tempyr`."""

    query: str = Field(min_length=1, max_length=500)
    scope: str | None = Field(default=None, min_length=1, max_length=200)
    max_results: int = Field(default=10, ge=1, le=50)


class TempyrGraphHit(StrictBaseModel):
    """One Tempyr graph search hit."""

    node_id: str
    node_type: str | None = None
    status: str | None = None
    title: str | None = None
    score: float | None = None
    snippet: str | None = None


class QueryTempyrResult(StrictBaseModel):
    """Tempyr graph query result."""

    query: str
    scope: str | None = None
    hits: list[TempyrGraphHit]


class TempyrRunner(Protocol):
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


def query_tempyr(
    request: QueryTempyrRequest,
    *,
    cwd: Path | None = None,
    runner: TempyrRunner = subprocess.run,
) -> QueryTempyrResult:
    """Run a bounded local Tempyr search and normalize the hits."""
    query_terms = [request.query]
    if request.scope:
        query_terms.insert(0, request.scope)
    completed = runner(
        [
            "tempyr",
            "search",
            "--json",
            "--max-results",
            str(request.max_results),
            *query_terms,
        ],
        cwd=cwd or Path.cwd(),
        text=True,
        capture_output=True,
        check=False,
    )
    if completed.returncode != 0:
        detail = completed.stderr.strip() or completed.stdout.strip()
        message = f"tempyr search failed: {detail}"
        raise RuntimeError(message)
    return QueryTempyrResult(
        query=request.query,
        scope=request.scope,
        hits=_parse_hits(completed.stdout),
    )


def _parse_hits(raw: str) -> list[TempyrGraphHit]:
    try:
        decoded: object = json.loads(raw)
    except json.JSONDecodeError as exc:
        message = f"tempyr search returned malformed JSON: {exc}"
        raise ValueError(message) from exc
    if not isinstance(decoded, list):
        message = "tempyr search JSON result was not a list"
        raise TypeError(message)
    hits = cast("list[object]", decoded)
    return [_parse_hit(item) for item in hits]


def _parse_hit(item: object) -> TempyrGraphHit:
    if not isinstance(item, dict):
        message = "tempyr search hit was not an object"
        raise TypeError(message)
    hit = cast("dict[str, object]", item)
    node_id = hit.get("node_id")
    if not isinstance(node_id, str) or not node_id:
        message = "tempyr search hit missing node_id"
        raise ValueError(message)
    return TempyrGraphHit(
        node_id=node_id,
        node_type=_optional_str(hit, "node_type"),
        status=_optional_str(hit, "status"),
        title=_optional_str(hit, "title"),
        score=_optional_float(hit, "score"),
        snippet=_optional_str(hit, "snippet"),
    )


def _optional_str(hit: dict[str, object], key: str) -> str | None:
    value = hit.get(key)
    return value if isinstance(value, str) else None


def _optional_float(hit: dict[str, object], key: str) -> float | None:
    value = hit.get(key)
    if isinstance(value, bool):
        return None
    if isinstance(value, int | float):
        return float(value)
    return None
