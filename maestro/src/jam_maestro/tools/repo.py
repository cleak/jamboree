"""Generated Pydantic models for tool service I/O."""

from __future__ import annotations

from typing import Literal

from pydantic import BaseModel, ConfigDict, Field


class StrictToolModel(BaseModel):
    """Base for closed tool contracts."""

    model_config = ConfigDict(extra="forbid", frozen=True, populate_by_name=True)


class FlexibleToolModel(BaseModel):
    """Base for open response contracts with service-owned extra fields."""

    model_config = ConfigDict(extra="allow", frozen=True, populate_by_name=True)


class RepoMarkReviewArtifactHandledRequest(StrictToolModel):
    """repo.mark-review-artifact-handled request."""

    artifact_id: str = Field(min_length=1)
    status: Literal["Open", "Acknowledged", "Addressed", "Dismissed"]
    reasoning: str = Field(min_length=1)


class RepoOpenPrRequest(StrictToolModel):
    """repo.open-pr request."""

    task_id: str = Field(min_length=1)
    branch: str = Field(min_length=1)
    title: str = Field(min_length=1)
    body: str | None = None
    draft: bool | None = None
    base: str | None = Field(default=None, min_length=1)
    repo: str | None = Field(default=None, min_length=1)
    worktree_path: str | None = Field(default=None, min_length=1)
    push: bool | None = None


class RepoPrStatusRequest(StrictToolModel):
    """repo.pr-status request."""

    pr_ref: str = Field(min_length=1)
    repo: str | None = Field(default=None, min_length=1)


class RepoPrepareMergeRequest(StrictToolModel):
    """repo.prepare-merge request."""

    pr_ref: str = Field(min_length=1)
    repo: str | None = Field(default=None, min_length=1)


class RepoReadPrCommentsRequest(StrictToolModel):
    """repo.read-pr-comments request."""

    pr_ref: str = Field(min_length=1)
    repo: str | None = Field(default=None, min_length=1)


class RepoReplyToCommentRequest(StrictToolModel):
    """repo.reply-to-comment request."""

    artifact_id: str = Field(min_length=1)
    text: str = Field(min_length=1)


class RepoRequestHumanMergeRequest(StrictToolModel):
    """repo.request-human-merge request."""

    pr_ref: str = Field(min_length=1)
    summary: str = Field(min_length=1)
    repo: str | None = Field(default=None, min_length=1)


class RepoRequestReviewRequest(StrictToolModel):
    """repo.request-review request."""

    pr_ref: str = Field(min_length=1)
    reviewer_id: str = Field(min_length=1)
    task_id: str | None = Field(default=None, min_length=1)
    repo: str | None = Field(default=None, min_length=1)
    worktree_path: str = Field(min_length=1)
    base: str | None = Field(default=None, min_length=1)


__all__ = [
    "RepoMarkReviewArtifactHandledRequest",
    "RepoOpenPrRequest",
    "RepoPrStatusRequest",
    "RepoPrepareMergeRequest",
    "RepoReadPrCommentsRequest",
    "RepoReplyToCommentRequest",
    "RepoRequestHumanMergeRequest",
    "RepoRequestReviewRequest",
]
