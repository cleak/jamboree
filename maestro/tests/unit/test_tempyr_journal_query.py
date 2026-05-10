from __future__ import annotations

import subprocess
from pathlib import Path

import pytest

from jam_maestro.tempyr_journal_query import (
    TempyrJournalBlameRequest,
    TempyrJournalRangeRequest,
    TempyrJournalSearchRequest,
    tempyr_journal_blame,
    tempyr_journal_range,
    tempyr_journal_search,
)


def test_tempyr_journal_search_runs_bounded_query_and_filters_agent() -> None:
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
                '{"count":2,"query":"trace","hits":['
                '{"entry":{"id":"j-1","agent":"maestro","kind":"decision"},"score":1.5},'
                '{"entry":{"id":"j-2","agent":"picker","kind":"dead_end"},"score":1.0}'
                "]}"
            ),
            stderr="",
        )

    result = tempyr_journal_search(
        TempyrJournalSearchRequest(
            query="trace",
            kind=["decision"],
            agent="maestro",
            since_days=7,
            limit=5,
            token_budget=1000,
        ),
        cwd=Path("/repo"),
        runner=runner,
    )

    assert calls == [
        [
            "tempyr",
            "journal",
            "search",
            "--json",
            "--limit",
            "5",
            "--kind",
            "decision",
            "--token-budget",
            "1000",
            "--since-days",
            "7",
            "trace",
        ]
    ]
    assert result.count == 1
    assert result.hits[0].entry["id"] == "j-1"


def test_tempyr_journal_range_and_blame_use_expected_subcommands() -> None:
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
        assert cwd == Path.cwd()
        assert text is True
        assert capture_output is True
        assert check is False
        return subprocess.CompletedProcess(
            args=args,
            returncode=0,
            stdout='{"count":0,"hits":[]}',
            stderr="",
        )

    tempyr_journal_range(
        TempyrJournalRangeRequest(rev_range="HEAD~1..HEAD", limit=3),
        runner=runner,
    )
    tempyr_journal_blame(
        TempyrJournalBlameRequest(file_path="crates/jam-cli/src/main.rs", limit=4),
        runner=runner,
    )

    assert calls == [
        ["tempyr", "journal", "range", "--json", "--limit", "3", "HEAD~1..HEAD"],
        [
            "tempyr",
            "journal",
            "blame",
            "--json",
            "--limit",
            "4",
            "crates/jam-cli/src/main.rs",
        ],
    ]


def test_tempyr_journal_query_fails_loudly_on_tempyr_error() -> None:
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

    with pytest.raises(RuntimeError, match="tempyr journal query failed: boom"):
        tempyr_journal_search(TempyrJournalSearchRequest(query="trace"), runner=runner)
