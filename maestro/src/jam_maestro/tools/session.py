"""Generated Pydantic models for tool service I/O."""

from __future__ import annotations

from pydantic import BaseModel, ConfigDict, Field


class StrictToolModel(BaseModel):
    """Base for closed tool contracts."""

    model_config = ConfigDict(extra="forbid", frozen=True, populate_by_name=True)


class FlexibleToolModel(BaseModel):
    """Base for open response contracts with service-owned extra fields."""

    model_config = ConfigDict(extra="allow", frozen=True, populate_by_name=True)


class SessionArchiveSessionRequest(StrictToolModel):
    """session.archive-session request."""

    session_id: str = Field(min_length=1)


class SessionFullStopRequest(StrictToolModel):
    """session.full-stop request."""

    session_id: str = Field(min_length=1)
    reason: str = Field(min_length=1)
    requested_by: str | None = Field(default=None, min_length=1)


class SessionInspectPickerRequest(StrictToolModel):
    """session.inspect-picker request."""

    session_id: str = Field(min_length=1)


class SessionListActiveRequest(StrictToolModel):
    """session.list-active request."""



class SessionPurgeSessionRequest(StrictToolModel):
    """session.purge-session request."""

    session_id: str = Field(min_length=1)
    reason: str = Field(min_length=1)
    preserve_worktree: bool | None = None


class SessionSpawnPickerRequest(StrictToolModel):
    """session.spawn-picker request."""

    task_id: str = Field(min_length=1)
    project: str | None = Field(default=None, min_length=1)
    harness: str | None = Field(default=None, min_length=1)
    sandbox_backend: str | None = Field(default=None, min_length=1)
    sandbox_profile: str | None = Field(default=None, min_length=1)
    task_class: str | None = Field(default=None, min_length=1)
    initial_prompt: str | None = Field(default=None, min_length=1)
    model_override: str | None = Field(default=None, min_length=1)
    reasoning_effort: str | None = Field(default=None, min_length=1)
    budget_usd: float | None = Field(default=None, ge=0)
    dry_run: bool | None = None


__all__ = [
    "SessionArchiveSessionRequest",
    "SessionFullStopRequest",
    "SessionInspectPickerRequest",
    "SessionListActiveRequest",
    "SessionPurgeSessionRequest",
    "SessionSpawnPickerRequest",
]
