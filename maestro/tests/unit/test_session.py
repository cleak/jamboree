from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING

import jam_maestro.session as session_module
from jam_maestro.input_budget import InputBudgetConfig
from jam_maestro.routing_manifest import RoutingManifest, RoutingManifestRouter
from jam_maestro.session import MaestroSessionLoop, NatsObserveClient, NatsSessionClient
from jam_maestro.skills import LoadedSkill, SkillScope
from jam_maestro.tempyr_journal import (
    DecisionEntry,
    JournalEntry,
    JournalFinalizeResult,
    OutcomeEntry,
)
from jam_maestro.tools import ObserveWorldSnapshotRequest, SessionSpawnPickerRequest
from jam_maestro.trace import TRACE_ID_LENGTH, new_trace_id
from jam_maestro.wake import TaskWake

if TYPE_CHECKING:
    import pytest

SESSION_TEST_SKILL_BYTES = 7


class FakeObserveClient:
    async def world_snapshot(
        self,
        request: ObserveWorldSnapshotRequest,
        trace_id: str,
    ) -> dict[str, object]:
        return {
            "task_id": request.task_id,
            "trace_id": trace_id,
            "readiness": {"status": "ready-with-warnings"},
        }


class ErrorObserveClient:
    async def world_snapshot(
        self,
        request: ObserveWorldSnapshotRequest,
        trace_id: str,
    ) -> dict[str, object]:
        _ = (request, trace_id)
        return {"error": {"kind": "not-implemented"}}


class FakeSkillLoader:
    async def load(self, scope: SkillScope) -> list[LoadedSkill]:
        assert scope.project == "blueberry"
        assert scope.task_class == "light-edit"
        return [LoadedSkill(path="/skills/Maestro.md", content="Maestro skill")]


class FakeJournal:
    def __init__(self) -> None:
        self.bootstrapped: list[str] = []
        self.decisions: list[DecisionEntry] = []
        self.outcomes: list[OutcomeEntry] = []
        self.finalized: list[str] = []

    async def bootstrap(self, agent: str) -> None:
        self.bootstrapped.append(agent)

    async def log_decision(self, agent: str, entry: DecisionEntry) -> None:
        assert agent.startswith("maestro:")
        self.decisions.append(entry)

    async def log_outcome(self, agent: str, entry: OutcomeEntry) -> None:
        assert agent.startswith("maestro:")
        assert "closed cleanly" in entry.detail
        assert entry.summary.startswith("Maestro session closed")
        assert entry.passed
        assert "task-maestro-session-loop" in entry.refs
        assert any(tag.startswith("trace:") for tag in entry.tags)
        self.outcomes.append(entry)

    async def log_entry(self, agent: str, entry: JournalEntry) -> None:
        if isinstance(entry, DecisionEntry):
            await self.log_decision(agent, entry)
        elif isinstance(entry, OutcomeEntry):
            await self.log_outcome(agent, entry)

    async def finalize(self, agent: str) -> JournalFinalizeResult:
        self.finalized.append(agent)
        return JournalFinalizeResult(flushed=True)


class FakeSessionClient:
    def __init__(self, response: dict[str, object]) -> None:
        self.response = response
        self.calls: list[tuple[SessionSpawnPickerRequest, str]] = []

    async def spawn_picker(
        self,
        request: SessionSpawnPickerRequest,
        trace_id: str,
    ) -> dict[str, object]:
        self.calls.append((request, trace_id))
        return self.response


class FakeTaskEvents:
    def __init__(self) -> None:
        self.failed: list[dict[str, object]] = []

    async def task_failed(
        self,
        wake: TaskWake,
        *,
        error_kind: str,
        detail: str,
        trace_id: str,
    ) -> None:
        self.failed.append(
            {
                "task_id": wake.task_id,
                "error_kind": error_kind,
                "detail": detail,
                "trace_id": trace_id,
            }
        )


def test_new_trace_id_is_ulid_shaped() -> None:
    trace_id = new_trace_id()
    assert len(trace_id) == TRACE_ID_LENGTH
    assert trace_id.isascii()


def test_session_loop_records_observed_decision() -> None:
    loop = MaestroSessionLoop(FakeObserveClient())
    decision = asyncio.run(loop.run_task_wake("task-1", "01HXKJ00000000000000000000"))

    assert decision.task_id == "task-1"
    assert decision.decision == "dispatch-ready:codex-cli"
    assert decision.dispatch is not None
    assert decision.world_snapshot["task_id"] == "task-1"
    assert decision.tempyr_agent.startswith("maestro:")


def test_session_loop_records_blocked_decision() -> None:
    loop = MaestroSessionLoop(ErrorObserveClient())
    decision = asyncio.run(loop.run_task_wake("task-1", "01HXKJ00000000000000000000"))

    assert decision.decision == "blocked:world-snapshot-error"


def test_session_loop_loads_skills_and_logs_tempyr_decision() -> None:
    journal = FakeJournal()
    loop = MaestroSessionLoop(
        FakeObserveClient(),
        skills=FakeSkillLoader(),
        journal=journal,
    )
    wake = TaskWake(
        trace_id="01HXKJ00000000000000000000",
        task_id="task-1",
        description="test task",
        project="blueberry",
        task_class="light-edit",
    )
    decision = asyncio.run(loop.run_task_wake(wake))

    assert decision.loaded_skills == ["/skills/Maestro.md"]
    assert decision.journal_flushed
    assert len(journal.decisions) == 1
    assert journal.decisions[0].chosen == "dispatch-ready:codex-cli"
    assert journal.finalized == [decision.tempyr_agent]


def test_session_loop_spawns_picker_when_session_client_present() -> None:
    journal = FakeJournal()
    session = FakeSessionClient(
        {
            "session_id": "codex-cli:test",
            "task_id": "task-1",
            "harness": "codex-cli",
        }
    )
    loop = MaestroSessionLoop(
        FakeObserveClient(),
        session=session,
        journal=journal,
    )

    decision = asyncio.run(loop.run_task_wake("task-1", "01HXKJ00000000000000000000"))

    assert decision.decision == "spawned:codex-cli"
    assert decision.picker_handle == {
        "session_id": "codex-cli:test",
        "task_id": "task-1",
        "harness": "codex-cli",
    }
    assert len(session.calls) == 1
    spawn_request, trace_id = session.calls[0]
    assert spawn_request.task_id == "task-1"
    assert spawn_request.harness == "codex-cli"
    assert trace_id == "01HXKJ00000000000000000000"
    decision_detail = journal.decisions[0].detail
    assert decision_detail is not None
    assert "spawn-picker returned session codex-cli:test" in decision_detail


def test_session_loop_blocks_when_spawn_picker_returns_error() -> None:
    task_events = FakeTaskEvents()
    session = FakeSessionClient(
        {
            "error": {
                "kind": "picker-launch-failed",
                "detail": "fake launch failure",
            }
        }
    )
    loop = MaestroSessionLoop(FakeObserveClient(), session=session, task_events=task_events)

    decision = asyncio.run(loop.run_task_wake("task-1", "01HXKJ00000000000000000000"))

    assert decision.decision == "blocked:spawn-picker-error"
    assert decision.picker_handle == {
        "error": {
            "kind": "picker-launch-failed",
            "detail": "fake launch failure",
        }
    }
    assert task_events.failed == [
        {
            "task_id": "task-1",
            "error_kind": "picker-launch-failed",
            "detail": "fake launch failure",
            "trace_id": "01HXKJ00000000000000000000",
        }
    ]


def test_session_loop_reports_budgeted_skill_input() -> None:
    loop = MaestroSessionLoop(
        FakeObserveClient(),
        skills=FakeSkillLoader(),
        input_budget=InputBudgetConfig(skill_files_max_bytes=SESSION_TEST_SKILL_BYTES),
    )
    wake = TaskWake(
        trace_id="01HXKJ00000000000000000000",
        task_id="task-1",
        description="test task",
        project="blueberry",
        task_class="light-edit",
    )

    decision = asyncio.run(loop.run_task_wake(wake))

    assert decision.loaded_skills == ["/skills/Maestro.md"]
    assert decision.input_budget.skill_bytes == SESSION_TEST_SKILL_BYTES
    assert decision.input_budget.skills_truncated == 1


def test_nats_observe_client_uses_routing_manifest(monkeypatch: pytest.MonkeyPatch) -> None:
    calls: list[dict[str, object]] = []

    async def fake_request_json(**kwargs: object) -> dict[str, object]:
        calls.append(dict(kwargs))
        return {"readiness": {"status": "ready"}}

    monkeypatch.setattr(session_module, "request_json", fake_request_json)
    routing = RoutingManifestRouter.with_manifest(
        RoutingManifest.model_validate(
            {
                "schema_version": 1,
                "updated_at": "2026-05-06T12:00:00Z",
                "updated_by": "human:caleb",
                "trace_id": "01HXKJ00000000000000000000",
                "services": {
                    "observe": {
                        "current_version": "0.4.7",
                        "subject_prefix": "tool.observe.v047",
                        "binary_path": "/home/maestro/.jam/bin/jam-svc-observe-0.4.7",
                        "binary_sha256": "abc123",
                        "started_at": "2026-05-06T12:00:00Z",
                        "expected_health": "ok",
                    }
                },
            }
        )
    )
    client = NatsObserveClient("nats://127.0.0.1:4222", routing=routing)

    response = asyncio.run(
        client.world_snapshot(
            ObserveWorldSnapshotRequest(task_id="task-1"),
            "01HXKJ00000000000000000000",
        )
    )

    assert response == {"readiness": {"status": "ready"}}
    assert calls[0]["subject"] == "tool.observe.v047.world-snapshot"


def test_nats_session_client_uses_routing_manifest(monkeypatch: pytest.MonkeyPatch) -> None:
    calls: list[dict[str, object]] = []

    async def fake_request_json(**kwargs: object) -> dict[str, object]:
        calls.append(dict(kwargs))
        return {"session_id": "codex-cli:test"}

    monkeypatch.setattr(session_module, "request_json", fake_request_json)
    routing = RoutingManifestRouter.with_manifest(
        RoutingManifest.model_validate(
            {
                "schema_version": 1,
                "updated_at": "2026-05-06T12:00:00Z",
                "updated_by": "human:caleb",
                "trace_id": "01HXKJ00000000000000000000",
                "services": {
                    "session": {
                        "current_version": "0.4.7",
                        "subject_prefix": "tool.session.v047",
                        "binary_path": "/home/maestro/.jam/bin/jam-svc-session-0.4.7",
                        "binary_sha256": "abc123",
                        "started_at": "2026-05-06T12:00:00Z",
                        "expected_health": "ok",
                    }
                },
            }
        )
    )
    client = NatsSessionClient("nats://127.0.0.1:4222", routing=routing)

    response = asyncio.run(
        client.spawn_picker(
            SessionSpawnPickerRequest(task_id="task-1", harness="codex-cli"),
            "01HXKJ00000000000000000000",
        )
    )

    assert response == {"session_id": "codex-cli:test"}
    assert calls[0]["subject"] == "tool.session.v047.spawn-picker"
