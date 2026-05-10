"""Quota-aware Picker dispatch policy."""

from __future__ import annotations

import re
from dataclasses import dataclass
from enum import StrEnum
from typing import TYPE_CHECKING, cast

from pydantic import Field

from jam_maestro.models import StrictBaseModel
from jam_maestro.tools import SessionSpawnPickerRequest

if TYPE_CHECKING:
    from collections.abc import Iterable, Mapping, Sequence

    from jam_maestro.skills import LoadedSkill
    from jam_maestro.wake import TaskWake

HARNESS_ALIASES: dict[str, str] = {
    "codex": "codex-cli",
    "codex-cli": "codex-cli",
    "claude": "claude-code",
    "claude-code": "claude-code",
    "deepseek": "opencode-deepseek",
    "opencode": "opencode-deepseek",
    "opencode-deepseek": "opencode-deepseek",
}

DEFAULT_TASK_PREFERENCES: dict[str, list[str]] = {
    "coderabbit-review": ["codex-cli", "claude-code", "opencode-deepseek"],
    "compile-heavy-rust": ["codex-cli", "claude-code", "opencode-deepseek"],
    "doc-generation": ["opencode-deepseek", "claude-code", "codex-cli"],
    "ecs-refactor": ["claude-code", "codex-cli", "opencode-deepseek"],
    "light-edit": ["codex-cli", "opencode-deepseek", "claude-code"],
    "risky-architecture": ["claude-code", "codex-cli", "opencode-deepseek"],
    "shader-variant": ["claude-code", "codex-cli", "opencode-deepseek"],
}

RELEVANT_WINDOWS: dict[str, set[str]] = {
    "codex-cli": {"local-messages"},
    "claude-code": {"rate-limit"},
    "opencode-deepseek": {"api-budget"},
}

DEFAULT_BUDGET_USD: dict[str, float] = {
    "coderabbit-review": 2.0,
    "compile-heavy-rust": 8.0,
    "doc-generation": 1.0,
    "ecs-refactor": 12.0,
    "light-edit": 2.0,
    "risky-architecture": 20.0,
    "shader-variant": 8.0,
}


class QuotaDisposition(StrEnum):
    """Routing view of a harness quota state."""

    AVAILABLE = "available"
    UNKNOWN = "unknown"
    LOW = "low"
    EXHAUSTED = "exhausted"


class DispatchChoice(StrictBaseModel):
    """A planned `spawn-picker` call chosen by the Maestro."""

    harness: str = Field(min_length=1)
    quota: QuotaDisposition
    reason: str = Field(min_length=1)
    spawn_request: SessionSpawnPickerRequest


class DispatchBlocked(StrictBaseModel):
    """Why dispatch could not proceed."""

    reason: str = Field(min_length=1)
    detail: str = Field(min_length=1)


@dataclass(frozen=True)
class _Candidate:
    harness: str
    preference_rank: int
    quota: QuotaDisposition
    quota_detail: str | None = None


@dataclass(frozen=True)
class _QuotaView:
    disposition: QuotaDisposition
    detail: str | None = None


def choose_dispatch(
    *,
    wake: TaskWake,
    world_snapshot: Mapping[str, object],
    skills: Sequence[LoadedSkill],
) -> DispatchChoice | DispatchBlocked:
    """Choose a harness from task-type skills and quota facts.

    Per §2.10 and `dec-three-tier-picker-pool`, subscription harnesses stay
    preferred when healthy, while API tier handles overflow and task classes
    that explicitly prefer it.
    """
    if "error" in world_snapshot:
        return DispatchBlocked(
            reason="world-snapshot-error",
            detail="world-snapshot returned error",
        )
    readiness = _readiness_status(world_snapshot)
    if readiness not in {"ready", "ready-with-warnings"}:
        return DispatchBlocked(
            reason="not-ready",
            detail=f"world-snapshot readiness is {readiness}",
        )

    task_class = wake.task_class or "light-edit"
    preferences = _harness_preferences(task_class, skills)
    quotas = _quota_map(world_snapshot)
    budget_usd = DEFAULT_BUDGET_USD.get(task_class, 2.0)
    candidates: list[_Candidate] = []
    for idx, harness in enumerate(preferences):
        quota = _quota_for_harness(harness, quotas, budget_usd)
        candidates.append(
            _Candidate(
                harness=harness,
                preference_rank=idx,
                quota=quota.disposition,
                quota_detail=quota.detail,
            )
        )
    usable = [
        candidate for candidate in candidates if candidate.quota != QuotaDisposition.EXHAUSTED
    ]
    if not usable:
        return DispatchBlocked(
            reason="all-candidate-quotas-exhausted",
            detail=_blocked_detail(candidates, preferences),
        )

    selected = min(
        usable,
        key=lambda candidate: (_quota_rank(candidate.quota), candidate.preference_rank),
    )
    return DispatchChoice(
        harness=selected.harness,
        quota=selected.quota,
        reason=_dispatch_reason(selected, preferences),
        spawn_request=_spawn_request(wake, selected.harness, task_class),
    )


def _harness_preferences(task_class: str, skills: Sequence[LoadedSkill]) -> list[str]:
    parsed: list[str] = []
    for skill in skills:
        if _is_task_type_skill(skill, task_class):
            parsed.extend(_canonical_harnesses(_parse_harness_line(skill.content)))
    fallback = DEFAULT_TASK_PREFERENCES.get(task_class, DEFAULT_TASK_PREFERENCES["light-edit"])
    return _dedupe([*parsed, *fallback])


def _is_task_type_skill(skill: LoadedSkill, task_class: str) -> bool:
    marker = f"task-types/{task_class}.md"
    return marker in skill.path or f"scope: task-types/{task_class}" in skill.content


def _parse_harness_line(content: str) -> list[str]:
    for line in content.splitlines():
        if re.search(r"\bharness\s*:", line):
            return [*re.findall(r'"([^"]+)"', line), *re.findall(r"`([^`]+)`", line)]
    return []


def _canonical_harnesses(raw_matches: Iterable[str]) -> list[str]:
    harnesses: list[str] = []
    for value in raw_matches:
        raw = value.strip().lower()
        if canonical := HARNESS_ALIASES.get(raw):
            harnesses.append(canonical)
    return harnesses


def _dedupe(values: Iterable[str]) -> list[str]:
    seen: set[str] = set()
    deduped: list[str] = []
    for value in values:
        if value in seen:
            continue
        seen.add(value)
        deduped.append(value)
    return deduped


def _quota_map(world_snapshot: Mapping[str, object]) -> dict[str, Mapping[str, object]]:
    raw = world_snapshot.get("harness_quotas")
    if not isinstance(raw, dict):
        return {}
    raw_map = cast("Mapping[object, object]", raw)
    quotas: dict[str, Mapping[str, object]] = {}
    for key, value in raw_map.items():
        if isinstance(key, str) and isinstance(value, dict):
            quotas[key] = cast("Mapping[str, object]", value)
    return quotas


def _quota_for_harness(
    harness: str,
    quotas: Mapping[str, Mapping[str, object]],
    budget_usd: float,
) -> _QuotaView:
    windows = _relevant_quota_windows(harness, quotas)
    if not windows:
        return _QuotaView(QuotaDisposition.UNKNOWN)
    statuses = {status for window in windows if isinstance((status := window.get("status")), str)}
    if "exhausted" in statuses:
        return _QuotaView(QuotaDisposition.EXHAUSTED)

    budget_view = _api_budget_quota_view(windows, budget_usd)
    if budget_view is not None:
        return budget_view

    return _quota_view_from_statuses(statuses)


def _api_budget_quota_view(
    windows: Sequence[Mapping[str, object]],
    budget_usd: float,
) -> _QuotaView | None:
    api_budget_remaining = _api_budget_remaining_usd(windows)
    if api_budget_remaining is None:
        return None
    if api_budget_remaining < budget_usd:
        return _QuotaView(
            QuotaDisposition.EXHAUSTED,
            (
                f"remaining API budget ${api_budget_remaining:.2f} is below "
                f"task budget ${budget_usd:.2f}"
            ),
        )
    if api_budget_remaining < budget_usd * 2:
        return _QuotaView(
            QuotaDisposition.LOW,
            (
                f"remaining API budget ${api_budget_remaining:.2f} is less than "
                f"2x task budget ${budget_usd:.2f}"
            ),
        )
    return None


def _quota_view_from_statuses(statuses: set[str]) -> _QuotaView:
    if "low" in statuses:
        return _QuotaView(QuotaDisposition.LOW)
    if "available" in statuses:
        return _QuotaView(QuotaDisposition.AVAILABLE)
    return _QuotaView(QuotaDisposition.UNKNOWN)


def _api_budget_remaining_usd(windows: Sequence[Mapping[str, object]]) -> float | None:
    remaining: list[float] = []
    for window in windows:
        api_budget = window.get("api_budget")
        if not isinstance(api_budget, dict):
            continue
        typed_api_budget = cast("Mapping[str, object]", api_budget)
        monthly_cap = _float_field(typed_api_budget, "monthly_cap_usd")
        spent = _float_field(typed_api_budget, "spent_this_month_usd")
        if monthly_cap is None or spent is None:
            continue
        remaining.append(max(0.0, monthly_cap - spent))
    return min(remaining) if remaining else None


def _float_field(value: Mapping[str, object], field: str) -> float | None:
    raw = value.get(field)
    if isinstance(raw, bool):
        return None
    if isinstance(raw, int | float):
        return float(raw)
    return None


def _relevant_quota_windows(
    harness: str,
    quotas: Mapping[str, Mapping[str, object]],
) -> list[Mapping[str, object]]:
    relevant = RELEVANT_WINDOWS.get(harness, set())
    by_harness = [
        value for key, value in quotas.items() if key == harness or key.startswith(f"{harness}/")
    ]
    if not relevant:
        return by_harness
    selected = [
        value
        for value in by_harness
        if isinstance(value.get("window_kind"), str)
        and cast("str", value["window_kind"]) in relevant
    ]
    return selected or by_harness


def _quota_rank(quota: QuotaDisposition) -> int:
    return {
        QuotaDisposition.AVAILABLE: 0,
        QuotaDisposition.UNKNOWN: 1,
        QuotaDisposition.LOW: 2,
        QuotaDisposition.EXHAUSTED: 99,
    }[quota]


def _dispatch_reason(selected: _Candidate, preferences: Sequence[str]) -> str:
    preference = "skill-preferred" if selected.preference_rank == 0 else "fallback"
    detail = f"; {selected.quota_detail}" if selected.quota_detail else ""
    return (
        f"{preference}: selected {selected.harness} with {selected.quota.value} quota "
        f"from candidates {', '.join(preferences)}{detail}"
    )


def _blocked_detail(candidates: Sequence[_Candidate], preferences: Sequence[str]) -> str:
    details = [candidate.quota_detail for candidate in candidates if candidate.quota_detail]
    if details:
        return f"all candidate harnesses exhausted: {', '.join(preferences)}; " + "; ".join(details)
    return f"all candidate harnesses exhausted: {', '.join(preferences)}"


def _spawn_request(wake: TaskWake, harness: str, task_class: str) -> SessionSpawnPickerRequest:
    return SessionSpawnPickerRequest(
        task_id=wake.task_id,
        project=wake.project,
        harness=harness,
        sandbox_backend="local",
        sandbox_profile="default",
        task_class=task_class,
        initial_prompt=_initial_prompt(wake, harness, task_class),
        model_override=_model_override(harness, task_class),
        budget_usd=DEFAULT_BUDGET_USD.get(task_class, 2.0),
    )


def _initial_prompt(wake: TaskWake, harness: str, task_class: str) -> str:
    return (
        f"Task: {wake.description}\n\n"
        "Project: Blueberry, the Bevy/Rust voxel game.\n"
        f"Task class: {task_class}.\n"
        f"Dispatch harness: {harness}.\n\n"
        "Apply the loaded Jamboree and Blueberry skills. Keep edits scoped to the task. "
        "Before opening a PR, run the relevant project validation gates and record any "
        "non-obvious decisions in the Tempyr journal."
    )


def _model_override(harness: str, task_class: str) -> str | None:
    if harness == "opencode-deepseek" and task_class in {"doc-generation", "light-edit"}:
        return "deepseek-v4-flash"
    if harness == "opencode-deepseek":
        return "deepseek-v4-pro"
    return None


def _readiness_status(snapshot: Mapping[str, object]) -> str:
    readiness = snapshot.get("readiness")
    if isinstance(readiness, dict):
        typed_readiness = cast("Mapping[str, object]", readiness)
        status = typed_readiness.get("status")
        if isinstance(status, str):
            return status
    return "unknown-readiness"
