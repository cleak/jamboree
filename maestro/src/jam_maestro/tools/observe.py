"""Generated Pydantic models for tool service I/O."""

from __future__ import annotations

from datetime import datetime  # noqa: TC003
from typing import Any

from pydantic import BaseModel, ConfigDict, Field


class StrictToolModel(BaseModel):
    """Base for closed tool contracts."""

    model_config = ConfigDict(extra="forbid", frozen=True, populate_by_name=True)


class FlexibleToolModel(BaseModel):
    """Base for open response contracts with service-owned extra fields."""

    model_config = ConfigDict(extra="allow", frozen=True, populate_by_name=True)


class ObserveBranchStalenessRequest(StrictToolModel):
    """observe.branch-staleness request."""

    worktree_path: str | None = Field(default=None, min_length=1)


class ObserveClassifyReviewArtifactsRequest(StrictToolModel):
    """observe.classify-review-artifacts request."""

    artifacts: list[Any]
    pr_ref: str | None = Field(default=None, min_length=1)


class ObserveComputeReadinessRequest(StrictToolModel):
    """observe.compute-readiness request."""

    task_id: str = Field(min_length=1)
    target: str | None = Field(default=None, min_length=1)
    max_staleness_secs: int | None = Field(default=None, ge=0)
    worktree_path: str | None = Field(default=None, min_length=1)


class ObserveListBlockersRequest(StrictToolModel):
    """observe.list-blockers request."""

    task_id: str = Field(min_length=1)
    target: str | None = Field(default=None, min_length=1)
    max_staleness_secs: int | None = Field(default=None, ge=0)
    worktree_path: str | None = Field(default=None, min_length=1)


class ObserveListReviewArtifactsRequest(StrictToolModel):
    """observe.list-review-artifacts request."""

    pr_ref: str | None = Field(default=None, min_length=1)
    status_filter: str | None = Field(default=None, min_length=1)


class ObserveQueryQuotaRequest(StrictToolModel):
    """observe.query-quota request."""

    harness_id: str | None = Field(default=None, min_length=1)


class ObserveRefreshWorldSnapshotRequest(StrictToolModel):
    """observe.refresh-world-snapshot request."""

    task_id: str = Field(min_length=1)
    target: str | None = Field(default=None, min_length=1)
    max_staleness_secs: int | None = Field(default=None, ge=0)
    worktree_path: str | None = Field(default=None, min_length=1)


class ObserveWorldSnapshotDeltaRequest(StrictToolModel):
    """observe.world-snapshot-delta request."""

    task_id: str = Field(min_length=1)
    target: str | None = Field(default=None, min_length=1)
    since: datetime | None = None
    max_staleness_secs: int | None = Field(default=None, ge=0)
    worktree_path: str | None = Field(default=None, min_length=1)


class ObserveWorldSnapshotDeltaResponse(StrictToolModel):
    """observe.world-snapshot-delta response."""

    task_id: str = Field(min_length=1)
    captured_at: datetime
    trace_id: str = Field(min_length=26, max_length=26)
    since: datetime | None = None
    baseline_captured_at: datetime | None = None
    full: bool
    reason: str = Field(min_length=1)
    changed_fields: dict[str, Any]


class ObserveWorldSnapshotRequest(StrictToolModel):
    """observe.world-snapshot request."""

    task_id: str = Field(min_length=1)
    target: str | None = Field(default=None, min_length=1)
    max_staleness_secs: int | None = Field(default=None, ge=0)
    worktree_path: str | None = Field(default=None, min_length=1)


class ObserveWorldSnapshotResponse(FlexibleToolModel):
    """observe.world-snapshot response."""

    task_id: str = Field(min_length=1)
    captured_at: datetime
    trace_id: str = Field(min_length=26, max_length=26)
    readiness: dict[str, Any]


__all__ = [
    "ObserveBranchStalenessRequest",
    "ObserveClassifyReviewArtifactsRequest",
    "ObserveComputeReadinessRequest",
    "ObserveListBlockersRequest",
    "ObserveListReviewArtifactsRequest",
    "ObserveQueryQuotaRequest",
    "ObserveRefreshWorldSnapshotRequest",
    "ObserveWorldSnapshotDeltaRequest",
    "ObserveWorldSnapshotDeltaResponse",
    "ObserveWorldSnapshotRequest",
    "ObserveWorldSnapshotResponse",
]
