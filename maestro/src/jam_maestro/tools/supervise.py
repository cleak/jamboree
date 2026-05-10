"""Generated Pydantic models for tool service I/O."""

from __future__ import annotations

from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field


class StrictToolModel(BaseModel):
    """Base for closed tool contracts."""

    model_config = ConfigDict(extra="forbid", frozen=True, populate_by_name=True)


class FlexibleToolModel(BaseModel):
    """Base for open response contracts with service-owned extra fields."""

    model_config = ConfigDict(extra="allow", frozen=True, populate_by_name=True)


class SuperviseNotifyHumanRequest(StrictToolModel):
    """supervise.notify-human request."""

    urgency: Literal["low", "medium", "high", "critical"] | None = None
    summary: str = Field(min_length=1, max_length=500)
    payload: dict[str, Any] | None = None


class SupervisePauseDispatchRequest(StrictToolModel):
    """supervise.pause-dispatch request."""

    reason: str = Field(min_length=1, max_length=500)
    changed_by: str | None = Field(default=None, min_length=1, max_length=200)


class SuperviseResumeDispatchRequest(StrictToolModel):
    """supervise.resume-dispatch request."""

    changed_by: str | None = Field(default=None, min_length=1, max_length=200)


__all__ = [
    "SuperviseNotifyHumanRequest",
    "SupervisePauseDispatchRequest",
    "SuperviseResumeDispatchRequest",
]
