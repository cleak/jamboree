from __future__ import annotations

import asyncio
from datetime import date
from pathlib import Path

from jam_maestro.record_learning import RecordLearningRequest, record_learning
from jam_maestro.tempyr_journal import (
    DecisionEntry,
    JournalEntry,
    JournalFinalizeResult,
    OutcomeEntry,
)
from jam_maestro.tool_registry import MaestroToolRegistry

TRACE_ID = "01HXKJVF7P4N6X5R8SRZWB6JCM"


class FakeJournal:
    def __init__(self) -> None:
        self.bootstrapped: list[str] = []
        self.decisions: list[DecisionEntry] = []
        self.finalized: list[str] = []

    async def bootstrap(self, agent: str) -> None:
        self.bootstrapped.append(agent)

    async def log_decision(self, agent: str, entry: DecisionEntry) -> None:
        self.decisions.append(entry)
        assert agent == "maestro:test"

    async def log_outcome(self, agent: str, entry: OutcomeEntry) -> None:
        _ = (agent, entry)

    async def log_entry(self, agent: str, entry: JournalEntry) -> None:
        if isinstance(entry, DecisionEntry):
            await self.log_decision(agent, entry)
        elif isinstance(entry, OutcomeEntry):
            await self.log_outcome(agent, entry)

    async def finalize(self, agent: str) -> JournalFinalizeResult:
        self.finalized.append(agent)
        return JournalFinalizeResult(flushed=True)


def test_record_learning_writes_skill_and_tempyr_decision(tmp_path: Path) -> None:
    journal = FakeJournal()
    request = RecordLearningRequest(
        scope="blueberry/coderabbit-extraction-suggestions",
        evidence="PR #1 and PR #2 both showed the same hot-path extraction issue.",
        guidance="Prefer a rationale reply over accepting extraction suggestions in hot paths.",
        counterexample="Cold-path setup code can still benefit from extraction.",
        confidence=0.7,
        originated_from_trace=TRACE_ID,
    )

    result = asyncio.run(
        record_learning(
            request,
            skills_root=tmp_path / "skills",
            journal=journal,
            agent="maestro:test",
            now=date(2026, 5, 6),
        )
    )

    skill_path = Path(result.skill_path)
    assert skill_path == (
        tmp_path / "skills/projects/blueberry/coderabbit-extraction-suggestions.md"
    )
    rendered = skill_path.read_text(encoding="utf-8")
    assert 'scope: "blueberry/coderabbit-extraction-suggestions"' in rendered
    assert f'originated-from-trace: "{TRACE_ID}"' in rendered
    assert "evidence: |-" in rendered
    assert "guidance: |-" in rendered
    assert f"`trace:{TRACE_ID}`" in rendered
    assert "`skill:blueberry/coderabbit-extraction-suggestions`" in rendered
    assert "### Counterexample" in rendered

    assert result.tempyr_logged
    assert result.journal_flushed
    assert journal.bootstrapped == ["maestro:test"]
    assert journal.finalized == ["maestro:test"]
    assert len(journal.decisions) == 1
    decision = journal.decisions[0]
    assert "projects/blueberry/coderabbit-extraction-suggestions.md" in decision.files
    assert "task-record-learning-tool" in decision.refs
    assert f"trace:{TRACE_ID}" in decision.tags
    assert "skill:blueberry/coderabbit-extraction-suggestions" in decision.tags


def test_record_learning_uses_unique_paths(tmp_path: Path) -> None:
    journal = FakeJournal()
    skills_root = tmp_path / "skills"
    existing = skills_root / "projects/blueberry/hot-paths.md"
    existing.parent.mkdir(parents=True)
    existing.write_text("existing", encoding="utf-8")

    result = asyncio.run(
        record_learning(
            RecordLearningRequest(
                scope="blueberry/hot-paths",
                evidence="Repeated profiling traces referenced the same hot path.",
                guidance="Keep the hot-path guidance separate from cold-path advice.",
                confidence=0.8,
                originated_from_trace=TRACE_ID,
            ),
            skills_root=skills_root,
            journal=journal,
            agent="maestro:test",
            now=date(2026, 5, 6),
        )
    )

    assert Path(result.skill_path).name == "hot-paths-2.md"
    assert existing.read_text(encoding="utf-8") == "existing"


def test_record_learning_is_registered_as_callable_tool() -> None:
    prepared = MaestroToolRegistry().prepare_request(
        "record-learning",
        {
            "scope": "blueberry/hot-paths",
            "evidence": "Two traces found the same issue.",
            "guidance": "Keep the rule scoped.",
            "confidence": 0.7,
            "originated_from_trace": TRACE_ID,
        },
    )

    assert prepared.route.subject == "meta.record-learning"
    assert isinstance(prepared.payload, RecordLearningRequest)
