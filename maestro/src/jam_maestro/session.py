"""Phase 1 Maestro session-loop scaffold."""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass
from datetime import UTC, datetime
from typing import Protocol, cast

from pydantic import Field

from jam_maestro.dispatch import DispatchBlocked, DispatchChoice, choose_dispatch
from jam_maestro.input_budget import (
    InputBudgetConfig,
    InputBudgetReport,
    assemble_session_input,
)
from jam_maestro.models import StrictBaseModel
from jam_maestro.nats_rpc import publish_json, request_json
from jam_maestro.routing_manifest import RoutingManifestRouter
from jam_maestro.skills import LoadedSkill, NullSkillLoader, SkillLoader, SkillScope
from jam_maestro.tempyr_journal import (
    DecisionEntry,
    JournalFinalizeResult,
    NullTempyrJournal,
    OutcomeEntry,
    TempyrJournalClient,
)
from jam_maestro.tools import ObserveWorldSnapshotRequest, SessionSpawnPickerRequest
from jam_maestro.trace import new_trace_id
from jam_maestro.wake import TaskWake


class ObserveClient(Protocol):
    """Observation tool client used by the session loop."""

    async def world_snapshot(
        self,
        request: ObserveWorldSnapshotRequest,
        trace_id: str,
    ) -> dict[str, object]:
        """Return the current world snapshot or a typed tool error envelope."""
        ...


class SessionClient(Protocol):
    """Session tool client used to launch a planned Picker."""

    async def spawn_picker(
        self,
        request: SessionSpawnPickerRequest,
        trace_id: str,
    ) -> dict[str, object]:
        """Launch a Picker or return a typed tool error envelope."""
        ...


class TaskEventPublisher(Protocol):
    """Publishes durable task lifecycle events."""

    async def task_failed(
        self,
        wake: TaskWake,
        *,
        error_kind: str,
        detail: str,
        trace_id: str,
    ) -> None:
        """Publish a task failure event."""
        ...


class NatsObserveClient:
    """NATS-backed observation tool client."""

    def __init__(
        self,
        nats_url: str = "nats://127.0.0.1:4222",
        *,
        routing: RoutingManifestRouter | None = None,
    ) -> None:
        self._nats_url = nats_url
        self._routing = routing or RoutingManifestRouter()

    async def world_snapshot(
        self,
        request: ObserveWorldSnapshotRequest,
        trace_id: str,
    ) -> dict[str, object]:
        """Call routed `observe.world-snapshot` with trace headers."""
        subject = await self._routing.subject_for("observe", "world-snapshot", trace_id)
        return await request_json(
            nats_url=self._nats_url,
            subject=subject,
            payload=request.model_dump(exclude_none=True),
            trace_id=trace_id,
        )


class NatsSessionClient:
    """NATS-backed session tool client."""

    def __init__(
        self,
        nats_url: str = "nats://127.0.0.1:4222",
        *,
        routing: RoutingManifestRouter | None = None,
    ) -> None:
        self._nats_url = nats_url
        self._routing = routing or RoutingManifestRouter()

    async def spawn_picker(
        self,
        request: SessionSpawnPickerRequest,
        trace_id: str,
    ) -> dict[str, object]:
        """Call routed `session.spawn-picker` with trace headers."""
        subject = await self._routing.subject_for("session", "spawn-picker", trace_id)
        return await request_json(
            nats_url=self._nats_url,
            subject=subject,
            payload=request.model_dump(exclude_none=True),
            trace_id=trace_id,
        )


class NatsTaskEventPublisher:
    """NATS-backed task lifecycle event publisher."""

    def __init__(self, nats_url: str = "nats://127.0.0.1:4222") -> None:
        self._nats_url = nats_url

    async def task_failed(
        self,
        wake: TaskWake,
        *,
        error_kind: str,
        detail: str,
        trace_id: str,
    ) -> None:
        """Publish `journal.task.failed` for failures before Picker start."""
        failed_at = datetime.now(UTC)
        envelope = {
            "schema_version": 1,
            "event_type": "task.failed",
            "event_subtype_version": 1,
            "timestamp": _rfc3339_z(failed_at),
            "journal_seq": 0,
            "trace_id": trace_id,
            "actor": "maestro",
            "payload": {
                "task_id": wake.task_id,
                "reason": error_kind,
                "detail": detail,
                "failed_at": _rfc3339_z(failed_at),
                "source_event_type": "maestro.spawn-picker-error",
            },
        }
        await publish_json(
            nats_url=self._nats_url,
            subject="journal.task.failed",
            payload=envelope,
            trace_id=trace_id,
        )


class SessionDecision(StrictBaseModel):
    """One scaffolded Maestro decision."""

    trace_id: str = Field(min_length=26, max_length=26)
    session_id: str
    tempyr_agent: str
    task_id: str
    loaded_skills: list[str]
    world_snapshot: dict[str, object]
    input_budget: InputBudgetReport
    decision: str
    dispatch: DispatchChoice | DispatchBlocked | None = None
    picker_handle: dict[str, object] | None = None
    journal_flushed: bool
    journal_flush_error: str | None = None


@dataclass(frozen=True, slots=True)
class _DecisionArtifacts:
    dispatch: DispatchChoice | DispatchBlocked | None
    picker_handle: dict[str, object] | None


class MaestroSessionLoop:
    """Minimal episodic loop: wake, call `world-snapshot`, record a decision."""

    def __init__(
        self,
        observe: ObserveClient,
        *,
        skills: SkillLoader | None = None,
        session: SessionClient | None = None,
        task_events: TaskEventPublisher | None = None,
        journal: TempyrJournalClient | None = None,
        input_budget: InputBudgetConfig | None = None,
    ) -> None:
        self._observe = observe
        self._skills = skills or NullSkillLoader()
        self._session = session
        self._task_events = task_events
        self._journal = journal or NullTempyrJournal()
        self._input_budget = input_budget or InputBudgetConfig()

    async def run_task_wake(
        self,
        wake: TaskWake | str,
        trace_id: str | None = None,
    ) -> SessionDecision:
        """Run one task wake without model/tool recursion yet."""
        task_wake = _coerce_wake(wake, trace_id)
        session_id = _new_session_id(task_wake.trace_id)
        tempyr_agent = f"maestro:{session_id}"
        scope = SkillScope(project=task_wake.project, task_class=task_wake.task_class)

        await self._journal.bootstrap(tempyr_agent)
        snapshot: dict[str, object] = {}
        input_bundle = assemble_session_input(
            wake=task_wake,
            world_snapshot=snapshot,
            skills=[],
            config=self._input_budget,
        )
        loaded_skills: list[LoadedSkill] = []
        dispatch: DispatchChoice | DispatchBlocked | None = None
        picker_handle: dict[str, object] | None = None
        decision = "blocked:session-incomplete"
        finalize_result = JournalFinalizeResult()
        try:
            raw_skills = await self._skills.load(scope)
            snapshot = await self._observe.world_snapshot(
                ObserveWorldSnapshotRequest(task_id=task_wake.task_id),
                task_wake.trace_id,
            )
            input_bundle = assemble_session_input(
                wake=task_wake,
                world_snapshot=snapshot,
                skills=raw_skills,
                config=self._input_budget,
            )
            loaded_skills = [
                LoadedSkill(path=skill.path, content=skill.content) for skill in input_bundle.skills
            ]
            dispatch = choose_dispatch(
                wake=task_wake,
                world_snapshot=snapshot,
                skills=loaded_skills,
            )
            decision = _decision_from_dispatch(snapshot, dispatch)
            if isinstance(dispatch, DispatchChoice) and self._session is not None:
                picker_handle = await self._session.spawn_picker(
                    dispatch.spawn_request,
                    task_wake.trace_id,
                )
                decision = _decision_from_picker(dispatch, picker_handle)
                if _picker_has_error(picker_handle) and self._task_events is not None:
                    await self._task_events.task_failed(
                        task_wake,
                        error_kind=_picker_error_kind(picker_handle),
                        detail=_picker_error_detail(picker_handle),
                        trace_id=task_wake.trace_id,
                    )
            await self._journal.log_decision(
                tempyr_agent,
                _decision_entry(
                    task_wake,
                    decision,
                    snapshot,
                    loaded_skills,
                    _DecisionArtifacts(dispatch=dispatch, picker_handle=picker_handle),
                ),
            )
            await self._journal.log_outcome(
                tempyr_agent,
                OutcomeEntry(
                    summary=f"Maestro session closed for task {task_wake.task_id}",
                    detail=(
                        f"Session {session_id} processed wake subject {task_wake.subject}; "
                        f"loaded {len(loaded_skills)} scoped skill file(s), called "
                        f"world-snapshot, recorded decision {decision}, and closed cleanly."
                    ),
                    passed="error" not in snapshot and not _picker_has_error(picker_handle),
                    tags=_journal_tags(task_wake),
                    refs=["task-maestro-session-loop"],
                ),
            )
        finally:
            finalize_result = await self._journal.finalize(tempyr_agent)

        return SessionDecision(
            trace_id=task_wake.trace_id,
            session_id=session_id,
            tempyr_agent=tempyr_agent,
            task_id=task_wake.task_id,
            loaded_skills=[skill.path for skill in loaded_skills],
            world_snapshot=snapshot,
            input_budget=input_bundle.report,
            decision=decision,
            dispatch=dispatch,
            picker_handle=picker_handle,
            journal_flushed=finalize_result.flushed,
            journal_flush_error=finalize_result.flush_error,
        )


def _coerce_wake(wake: TaskWake | str, trace_id: str | None) -> TaskWake:
    if isinstance(wake, TaskWake):
        return wake
    return TaskWake(
        trace_id=trace_id or new_trace_id(),
        task_id=wake,
        description=f"manual wake for {wake}",
    )


def _decision_from_dispatch(
    snapshot: dict[str, object],
    dispatch: DispatchChoice | DispatchBlocked | None,
) -> str:
    if "error" in snapshot:
        return "blocked:world-snapshot-error"
    if isinstance(dispatch, DispatchChoice):
        return f"dispatch-ready:{dispatch.harness}"
    if isinstance(dispatch, DispatchBlocked):
        return f"blocked:{dispatch.reason}"
    return f"observed:{_readiness_status(snapshot)}"


def _decision_from_picker(
    dispatch: DispatchChoice,
    picker_handle: dict[str, object],
) -> str:
    if _picker_has_error(picker_handle):
        return "blocked:spawn-picker-error"
    return f"spawned:{dispatch.harness}"


def _picker_has_error(picker_handle: dict[str, object] | None) -> bool:
    return isinstance(picker_handle, dict) and isinstance(picker_handle.get("error"), dict)


def _new_session_id(trace_id: str) -> str:
    now = datetime.now(UTC).strftime("%Y-%m-%d-%H-%M-%S")
    return f"maestro-{now}-{trace_id[-6:].lower()}"


def _decision_entry(
    wake: TaskWake,
    decision: str,
    snapshot: dict[str, object],
    loaded_skills: list[LoadedSkill],
    artifacts: _DecisionArtifacts,
) -> DecisionEntry:
    readiness = _readiness_status(snapshot)
    skill_list = (
        ", ".join(_journal_safe_skill_path(skill.path) for skill in loaded_skills) or "none"
    )
    return DecisionEntry(
        summary=f"Maestro chose {decision} for task {wake.task_id}",
        chosen=decision,
        rationale=(
            f"The Maestro started from world-snapshot for {wake.task_id}; "
            f"readiness is {readiness}; scoped skills loaded: {len(loaded_skills)}."
        ),
        detail=(
            f"Wake source {wake.subject} requested task {wake.task_id} "
            f"({wake.project}/{wake.task_class or 'unspecified-task-class'}) with "
            f"priority {wake.priority}. Loaded skill files: {skill_list}. "
            f"{_dispatch_detail(artifacts.dispatch, artifacts.picker_handle)}"
        ),
        reversible=True,
        tags=_journal_tags(wake),
        refs=["task-maestro-session-loop", "task-dispatch-policy-quota-skill-driven"],
    )


def _dispatch_detail(
    dispatch: DispatchChoice | DispatchBlocked | None,
    picker_handle: dict[str, object] | None,
) -> str:
    if isinstance(dispatch, DispatchChoice):
        detail = (
            f"Dispatch policy planned spawn-picker with harness {dispatch.harness}; "
            f"quota disposition {dispatch.quota.value}; reason: {dispatch.reason}."
        )
        if picker_handle is None:
            return detail
        if _picker_has_error(picker_handle):
            return f"{detail} spawn-picker returned error {_picker_error_kind(picker_handle)}."
        return f"{detail} spawn-picker returned session {_picker_session_id(picker_handle)}."
    if isinstance(dispatch, DispatchBlocked):
        return f"Dispatch policy blocked spawn-picker: {dispatch.reason} ({dispatch.detail})."
    return "Dispatch policy did not produce a spawn plan."


def _picker_error_kind(picker_handle: dict[str, object]) -> str:
    error = picker_handle.get("error")
    if isinstance(error, Mapping):
        error_map = cast("Mapping[str, object]", error)
        kind = error_map.get("kind")
        if isinstance(kind, str) and kind:
            return kind
    return "unknown"


def _picker_error_detail(picker_handle: dict[str, object] | None) -> str:
    if not isinstance(picker_handle, dict):
        return "spawn-picker did not return a response"
    error = picker_handle.get("error")
    if isinstance(error, Mapping):
        error_map = cast("Mapping[str, object]", error)
        detail = error_map.get("detail")
        if isinstance(detail, str) and detail.strip():
            return detail
    return f"spawn-picker failed with {_picker_error_kind(picker_handle)}"


def _picker_session_id(picker_handle: dict[str, object]) -> str:
    session_id = picker_handle.get("session_id")
    if isinstance(session_id, str) and session_id:
        return session_id
    return "unknown-session"


def _rfc3339_z(value: datetime) -> str:
    return value.astimezone(UTC).isoformat().replace("+00:00", "Z")


def _journal_tags(wake: TaskWake) -> list[str]:
    tags = [
        f"trace:{wake.trace_id}",
        f"task:{wake.task_id}",
        f"project:{wake.project}",
    ]
    if wake.task_class:
        tags.append(f"task-class:{wake.task_class}")
    if wake.parent_trace_id:
        tags.append(f"parent-trace:{wake.parent_trace_id}")
    return tags


def _journal_safe_skill_path(path: str) -> str:
    if "/skills/" in path:
        return f"skills/{path.split('/skills/', 1)[1]}"
    if path.startswith("/home/caleb/blueberry/"):
        return f"blueberry/{path.removeprefix('/home/caleb/blueberry/')}"
    return path.rsplit("/", 1)[-1]


def _readiness_status(snapshot: dict[str, object]) -> str:
    readiness = snapshot.get("readiness")
    if isinstance(readiness, Mapping):
        typed_readiness = cast("Mapping[object, object]", readiness)
        status = typed_readiness.get("status")
        if isinstance(status, str):
            return status
    return "unknown-readiness"
