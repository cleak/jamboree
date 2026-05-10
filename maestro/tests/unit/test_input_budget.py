from __future__ import annotations

from typing import TYPE_CHECKING

from jam_maestro.input_budget import (
    InputBudgetConfig,
    assemble_session_input,
    load_input_budget_config,
)
from jam_maestro.skills import LoadedSkill
from jam_maestro.wake import TaskWake

if TYPE_CHECKING:
    from pathlib import Path

PER_SESSION_INPUT_TOKENS = 1_234
SKILL_FILES_MAX_BYTES = 12
JOURNAL_REPLAY_MAX_EVENTS = 2
WORLD_SNAPSHOT_MAX_BYTES = 34


def test_load_input_budget_config_reads_maestro_toml(tmp_path: Path) -> None:
    config = tmp_path / "maestro.toml"
    config.write_text(
        """
[budget]
per-session-input-tokens = 1234

[input-budget]
skill-files-max-bytes = 12
journal-replay-max-events = 2
world-snapshot-max-bytes = 34
""",
        encoding="utf-8",
    )

    loaded = load_input_budget_config(config)

    assert loaded.per_session_input_tokens == PER_SESSION_INPUT_TOKENS
    assert loaded.skill_files_max_bytes == SKILL_FILES_MAX_BYTES
    assert loaded.journal_replay_max_events == JOURNAL_REPLAY_MAX_EVENTS
    assert loaded.world_snapshot_max_bytes == WORLD_SNAPSHOT_MAX_BYTES


def test_assemble_session_input_truncates_skills_and_limits_journal() -> None:
    bundle = assemble_session_input(
        wake=_wake(),
        world_snapshot={"task_id": "task-1", "readiness": {"status": "ready"}},
        skills=[
            LoadedSkill(path="/skills/a.md", content="abcdefghij"),
            LoadedSkill(path="/skills/b.md", content="klmnopqrst"),
        ],
        journal_events=[{"seq": 1}, {"seq": 2}, {"seq": 3}],
        config=InputBudgetConfig(
            skill_files_max_bytes=SKILL_FILES_MAX_BYTES,
            journal_replay_max_events=JOURNAL_REPLAY_MAX_EVENTS,
            world_snapshot_max_bytes=1_000,
        ),
    )

    assert [skill.path for skill in bundle.skills] == ["/skills/a.md", "/skills/b.md"]
    assert bundle.skills[0].content == "abcdefghij"
    assert bundle.skills[1].content == "kl"
    assert bundle.skills[1].truncated
    assert len(bundle.journal_events) == JOURNAL_REPLAY_MAX_EVENTS
    assert bundle.report.skills_truncated == 1
    assert bundle.report.journal_events_dropped == 1
    assert "journal_replay_limited" in bundle.report.warnings


def test_world_snapshot_stays_when_total_budget_is_tight() -> None:
    bundle = assemble_session_input(
        wake=_wake(),
        world_snapshot={"task_id": "task-1", "large": "x" * 200},
        skills=[LoadedSkill(path="/skills/a.md", content="skill content")],
        journal_events=[{"seq": 1, "body": "journal"}],
        config=InputBudgetConfig(
            per_session_input_tokens=1,
            skill_files_max_bytes=100,
            journal_replay_max_events=10,
            world_snapshot_max_bytes=1_000,
        ),
    )

    assert bundle.world_snapshot
    assert bundle.skills == []
    assert bundle.journal_events == []
    assert "wake_context_and_world_snapshot_exceed_input_budget" in bundle.report.warnings


def _wake() -> TaskWake:
    return TaskWake(
        trace_id="01HXKJ00000000000000000000",
        task_id="task-1",
        description="test wake",
        project="blueberry",
        task_class="light-edit",
    )
