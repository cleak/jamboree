from __future__ import annotations

from jam_maestro.dispatch import (
    DispatchBlocked,
    DispatchChoice,
    QuotaDisposition,
    choose_dispatch,
)
from jam_maestro.skills import LoadedSkill
from jam_maestro.wake import TaskWake

TRACE = "01HXKJ00000000000000000000"


def test_dispatch_prefers_skill_harness_when_quota_available() -> None:
    dispatch = choose_dispatch(
        wake=_wake("light-edit"),
        world_snapshot=_snapshot(
            {
                "codex-cli/local-messages": _quota("available", "local-messages"),
                "opencode-deepseek/api-budget": _quota("available", "api-budget"),
            }
        ),
        skills=[
            LoadedSkill(
                path="/skills/task-types/light-edit.md",
                content='scope: task-types/light-edit\nspawn-picker(spec={harness: "codex-cli"})',
            )
        ],
    )

    assert isinstance(dispatch, DispatchChoice)
    assert dispatch.harness == "codex-cli"
    assert dispatch.quota == QuotaDisposition.AVAILABLE
    assert dispatch.spawn_request.harness == "codex-cli"
    assert dispatch.spawn_request.task_class == "light-edit"


def test_dispatch_falls_back_when_preferred_harness_exhausted() -> None:
    dispatch = choose_dispatch(
        wake=_wake("light-edit"),
        world_snapshot=_snapshot(
            {
                "codex-cli/local-messages": _quota("exhausted", "local-messages"),
                "opencode-deepseek/api-budget": _quota("available", "api-budget"),
            }
        ),
        skills=[
            LoadedSkill(
                path="/skills/task-types/light-edit.md",
                content='scope: task-types/light-edit\nspawn-picker(spec={harness: "codex-cli"})',
            )
        ],
    )

    assert isinstance(dispatch, DispatchChoice)
    assert dispatch.harness == "opencode-deepseek"
    assert dispatch.spawn_request.model_override == "deepseek-v4-flash"
    assert "fallback" in dispatch.reason


def test_dispatch_deprioritizes_low_quota_but_keeps_it_usable() -> None:
    dispatch = choose_dispatch(
        wake=_wake("compile-heavy-rust"),
        world_snapshot=_snapshot(
            {
                "codex-cli/local-messages": _quota("low", "local-messages"),
                "claude-code/rate-limit": _quota("available", "rate-limit"),
            }
        ),
        skills=[
            LoadedSkill(
                path="/skills/task-types/compile-heavy-rust.md",
                content=(
                    "scope: task-types/compile-heavy-rust\n"
                    'spawn-picker(spec={harness: "codex-cli" | "claude-code"})'
                ),
            )
        ],
    )

    assert isinstance(dispatch, DispatchChoice)
    assert dispatch.harness == "claude-code"
    assert dispatch.quota == QuotaDisposition.AVAILABLE


def test_dispatch_skips_api_harness_when_remaining_budget_is_too_small() -> None:
    dispatch = choose_dispatch(
        wake=_wake("doc-generation"),
        world_snapshot=_snapshot(
            {
                "opencode-deepseek/api-budget": _api_quota(
                    status="available",
                    monthly_cap_usd=10.0,
                    spent_this_month_usd=9.25,
                ),
                "claude-code/rate-limit": _quota("available", "rate-limit"),
            }
        ),
        skills=[
            LoadedSkill(
                path="/skills/task-types/doc-generation.md",
                content=(
                    "scope: task-types/doc-generation\n"
                    'spawn-picker(spec={harness: "opencode-deepseek" | "claude-code"})'
                ),
            )
        ],
    )

    assert isinstance(dispatch, DispatchChoice)
    assert dispatch.harness == "claude-code"
    assert "remaining API budget $0.75 is below task budget $1.00" not in dispatch.reason


def test_dispatch_marks_api_harness_low_when_remaining_budget_is_tight() -> None:
    dispatch = choose_dispatch(
        wake=_wake("doc-generation"),
        world_snapshot=_snapshot(
            {
                "opencode-deepseek/api-budget": _api_quota(
                    status="available",
                    monthly_cap_usd=10.0,
                    spent_this_month_usd=8.50,
                ),
                "claude-code/rate-limit": _quota("unknown", "rate-limit"),
            }
        ),
        skills=[
            LoadedSkill(
                path="/skills/task-types/doc-generation.md",
                content=(
                    'scope: task-types/doc-generation\nspawn-picker(spec={harness: "opencode"})'
                ),
            )
        ],
    )

    assert isinstance(dispatch, DispatchChoice)
    assert dispatch.harness == "claude-code"
    assert dispatch.quota == QuotaDisposition.UNKNOWN


def test_dispatch_blocks_when_all_candidates_exhausted() -> None:
    dispatch = choose_dispatch(
        wake=_wake("light-edit"),
        world_snapshot=_snapshot(
            {
                "codex-cli/local-messages": _quota("exhausted", "local-messages"),
                "opencode-deepseek/api-budget": _quota("exhausted", "api-budget"),
                "claude-code/rate-limit": _quota("exhausted", "rate-limit"),
            }
        ),
        skills=[],
    )

    assert isinstance(dispatch, DispatchBlocked)
    assert dispatch.reason == "all-candidate-quotas-exhausted"


def test_dispatch_block_detail_includes_api_budget_reason() -> None:
    dispatch = choose_dispatch(
        wake=_wake("light-edit"),
        world_snapshot=_snapshot(
            {
                "codex-cli/local-messages": _quota("exhausted", "local-messages"),
                "opencode-deepseek/api-budget": _api_quota(
                    status="available",
                    monthly_cap_usd=2.50,
                    spent_this_month_usd=1.00,
                ),
                "claude-code/rate-limit": _quota("exhausted", "rate-limit"),
            }
        ),
        skills=[],
    )

    assert isinstance(dispatch, DispatchBlocked)
    assert dispatch.reason == "all-candidate-quotas-exhausted"
    assert "remaining API budget $1.50 is below task budget $2.00" in dispatch.detail


def test_dispatch_blocks_not_ready_snapshot() -> None:
    dispatch = choose_dispatch(
        wake=_wake("light-edit"),
        world_snapshot={"readiness": {"status": "not-ready"}, "harness_quotas": {}},
        skills=[],
    )

    assert isinstance(dispatch, DispatchBlocked)
    assert dispatch.reason == "not-ready"


def _wake(task_class: str) -> TaskWake:
    return TaskWake(
        trace_id=TRACE,
        task_id="task-1",
        description="do work",
        project="blueberry",
        task_class=task_class,
    )


def _snapshot(quotas: dict[str, dict[str, object]]) -> dict[str, object]:
    return {
        "readiness": {"status": "ready-with-warnings"},
        "harness_quotas": quotas,
    }


def _quota(status: str, window_kind: str) -> dict[str, object]:
    return {
        "status": status,
        "window_kind": window_kind,
        "detail": f"{window_kind} {status}",
        "source": "test",
        "observed_at": "2026-05-06T10:00:00Z",
    }


def _api_quota(
    *,
    status: str,
    monthly_cap_usd: float,
    spent_this_month_usd: float,
) -> dict[str, object]:
    quota = _quota(status, "api-budget")
    quota["api_budget"] = {
        "provider": "deepseek",
        "model": "deepseek-v4-pro",
        "monthly_cap_usd": monthly_cap_usd,
        "spent_this_month_usd": spent_this_month_usd,
        "current_input_rate_per_1m": 0.14,
        "current_output_rate_per_1m": 0.28,
    }
    return quota
