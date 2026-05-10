from __future__ import annotations

import subprocess
from pathlib import Path

import pytest

from jam_maestro.tempyr_query import QueryTempyrRequest, query_tempyr


def test_query_tempyr_runs_bounded_search() -> None:
    calls: list[list[str]] = []

    def runner(
        args: list[str],
        *,
        cwd: Path,
        text: bool,
        capture_output: bool,
        check: bool,
    ) -> subprocess.CompletedProcess[str]:
        calls.append(args)
        assert cwd == Path("/repo")
        assert text is True
        assert capture_output is True
        assert check is False
        return subprocess.CompletedProcess(
            args=args,
            returncode=0,
            stdout=(
                '[{"node_id":"api-query-tempyr","node_type":"api_surface",'
                '"status":"stable","title":"api-query-tempyr","score":1.2,'
                '"snippet":"query-tempyr"}]'
            ),
            stderr="",
        )

    result = query_tempyr(
        QueryTempyrRequest(query="Tempyr graph", scope="blueberry", max_results=3),
        cwd=Path("/repo"),
        runner=runner,
    )

    assert calls == [
        ["tempyr", "search", "--json", "--max-results", "3", "blueberry", "Tempyr graph"]
    ]
    assert result.hits[0].node_id == "api-query-tempyr"
    assert result.hits[0].status == "stable"


def test_query_tempyr_fails_loudly_on_tempyr_error() -> None:
    def runner(
        args: list[str],
        *,
        cwd: Path,
        text: bool,
        capture_output: bool,
        check: bool,
    ) -> subprocess.CompletedProcess[str]:
        assert cwd == Path.cwd()
        assert text is True
        assert capture_output is True
        assert check is False
        return subprocess.CompletedProcess(args=args, returncode=1, stdout="", stderr="boom")

    with pytest.raises(RuntimeError, match="tempyr search failed: boom"):
        query_tempyr(QueryTempyrRequest(query="Tempyr graph"), runner=runner)
