"""Generated Pydantic models for tool service I/O."""

from __future__ import annotations

from pydantic import BaseModel, ConfigDict, Field


class StrictToolModel(BaseModel):
    """Base for closed tool contracts."""

    model_config = ConfigDict(extra="forbid", frozen=True, populate_by_name=True)


class FlexibleToolModel(BaseModel):
    """Base for open response contracts with service-owned extra fields."""

    model_config = ConfigDict(extra="allow", frozen=True, populate_by_name=True)


class MessageEnqueueMessageRequest(StrictToolModel):
    """message.enqueue-message request."""

    session_id: str = Field(min_length=1, max_length=128)
    text: str = Field(min_length=1, max_length=8000)
    from_: str | None = Field(default=None, alias="from", min_length=1, max_length=128)


class MessageFullStopRequest(StrictToolModel):
    """message.full-stop request."""

    session_id: str = Field(min_length=1, max_length=128)
    reason: str = Field(min_length=1, max_length=8000)
    from_: str | None = Field(default=None, alias="from", min_length=1, max_length=128)
    requested_by: str | None = Field(default=None, min_length=1, max_length=128)


class MessageInterruptWithMessageRequest(StrictToolModel):
    """message.interrupt-with-message request."""

    session_id: str = Field(min_length=1, max_length=128)
    text: str = Field(min_length=1, max_length=8000)
    from_: str | None = Field(default=None, alias="from", min_length=1, max_length=128)


__all__ = [
    "MessageEnqueueMessageRequest",
    "MessageFullStopRequest",
    "MessageInterruptWithMessageRequest",
]
