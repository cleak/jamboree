from __future__ import annotations

import asyncio

from jam_maestro.tempyr_journal import (
    AssumptionEntry,
    CliTempyrJournal,
    DeadEndEntry,
    DecisionEntry,
    FindingEntry,
    OutcomeEntry,
    PlanEntry,
    QuestionEntry,
    RiskEntry,
)


class RecordingJournal(CliTempyrJournal):
    def __init__(self) -> None:
        self.calls: list[list[str]] = []

    async def _run(self, *args: str) -> None:
        self.calls.append(list(args))


def test_cli_tempyr_journal_logs_all_entry_kinds() -> None:
    journal = RecordingJournal()

    asyncio.run(
        journal.log_entry(
            "maestro:test",
            PlanEntry(
                summary="Plan the next bounded implementation slice",
                detail="Check local contracts before touching runtime setup.",
                tags=["trace:01HXKJ00000000000000000000"],
            ),
        )
    )
    asyncio.run(
        journal.log_entry(
            "maestro:test",
            FindingEntry(summary="Found an implemented local Tempyr query surface"),
        )
    )
    asyncio.run(
        journal.log_entry(
            "maestro:test",
            AssumptionEntry(
                summary="Assume local graph validation is sufficient",
                polarity="positive",
            ),
        )
    )
    asyncio.run(
        journal.log_entry(
            "maestro:test",
            QuestionEntry(summary="Should session archive delete worktrees or retain them"),
        )
    )
    asyncio.run(
        journal.log_entry(
            "maestro:test",
            DecisionEntry(
                summary="Use Tempyr CLI as journal query backend",
                chosen="tempyr journal --json",
                rationale="It is the supported local Tempyr interface.",
                detail=(
                    "The Maestro should invoke Tempyr directly without a shell and keep the "
                    "result shape typed at its boundary."
                ),
                alternatives=["custom SQLite reader"],
                reversible=True,
            ),
        )
    )
    asyncio.run(
        journal.log_entry(
            "maestro:test",
            DeadEndEntry(
                summary="Avoid guessing archive-session cleanup semantics",
                approach="Implement process-local cleanup immediately",
                failure_mode="The graph does not define whether artifacts move before deletion.",
                detail=(
                    "Session archive and purge touch lifecycle policy, so they need a policy "
                    "decision before implementation."
                ),
                next_to_try="Ask for lifecycle semantics or inspect a future runbook.",
            ),
        )
    )
    asyncio.run(
        journal.log_entry(
            "maestro:test",
            RiskEntry(
                summary="Runtime substrate is still not installed",
                severity="blocker",
            ),
        )
    )
    asyncio.run(
        journal.log_entry(
            "maestro:test",
            OutcomeEntry(
                summary="Typed journal entry coverage completed",
                detail="All Tempyr journal entry kinds can be logged through one wrapper.",
                passed=True,
                final=True,
            ),
        )
    )

    kinds = [call[4] for call in journal.calls]
    assert kinds == [
        "plan",
        "finding",
        "assumption",
        "question",
        "decision",
        "dead_end",
        "risk",
        "outcome",
    ]
    assert "--polarity" in journal.calls[2]
    assert "--alternative" in journal.calls[4]
    assert "--failure-mode" in journal.calls[5]
    assert "--severity" in journal.calls[6]
    assert "--final" in journal.calls[7]
