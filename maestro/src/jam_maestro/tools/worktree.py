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


class WorktreeCreateRequest(StrictToolModel):
    """worktree.create request."""

    task_id: str = Field(min_length=1)
    project: str | None = Field(default=None, min_length=1)
    repo_path: str | None = Field(default=None, min_length=1)
    worktree_root: str | None = Field(default=None, min_length=1)
    trunk_branch: str | None = Field(default=None, min_length=1)


class WorktreeCreateResponse(StrictToolModel):
    """worktree.create response."""

    task_id: str = Field(min_length=1)
    project: str = Field(min_length=1)
    repo_path: str = Field(min_length=1)
    worktree_path: str = Field(min_length=1)
    branch: str = Field(min_length=1)
    trunk_ref: str = Field(min_length=1)
    trunk_sha: str = Field(min_length=1)
    fetched: bool
    branched_at: datetime
    fetch_cursor_at_create: datetime
    trace_id: str = Field(min_length=26, max_length=26)


class WorktreeFindConflictsRequest(StrictToolModel):
    """worktree.find-conflicts request."""

    worktree_path: str = Field(min_length=1)
    target_ref: str = Field(min_length=1)


class WorktreeFindConflictsResponse(StrictToolModel):
    """worktree.find-conflicts response."""

    worktree_path: str = Field(min_length=1)
    target_ref: str = Field(min_length=1)
    conflicting_paths: list[Any]


class WorktreeWorktreeDiffRequest(StrictToolModel):
    """worktree.worktree-diff request."""

    worktree_path: str = Field(min_length=1)
    base_ref: str | None = Field(default=None, min_length=1)


class WorktreeWorktreeDiffResponse(StrictToolModel):
    """worktree.worktree-diff response."""

    worktree_path: str = Field(min_length=1)
    base_ref: str = Field(min_length=1)
    changed_files: list[Any]
    diff: str


__all__ = [
    "WorktreeCreateRequest",
    "WorktreeCreateResponse",
    "WorktreeFindConflictsRequest",
    "WorktreeFindConflictsResponse",
    "WorktreeWorktreeDiffRequest",
    "WorktreeWorktreeDiffResponse",
]
