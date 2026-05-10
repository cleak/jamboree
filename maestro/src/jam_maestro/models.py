"""Pydantic models for the Maestro backend boundary."""

from __future__ import annotations

from typing import Annotated, Literal

from pydantic import BaseModel, ConfigDict, Field

TraceId = Annotated[str, Field(pattern=r"^[0-9A-HJKMNP-TV-Z]{26}$")]
Role = Literal["system", "user", "assistant", "tool"]
ReasoningEffort = Literal["low", "medium", "high", "xhigh"]
StopReason = Literal["end_turn", "tool_use", "max_tokens", "content_filter", "error", "other"]


class StrictBaseModel(BaseModel):
    """Base model that rejects contract drift at the Python boundary."""

    model_config = ConfigDict(extra="forbid", frozen=True)


class Message(StrictBaseModel):
    """A chat message sent to the backend."""

    role: Role
    content: str = Field(min_length=1)


class FunctionSpec(StrictBaseModel):
    """LiteLLM-compatible function tool schema."""

    name: str = Field(pattern=r"^[A-Za-z0-9_.-]+$", max_length=128)
    description: str
    parameters: dict[str, object] = Field(default_factory=dict)


class ToolDefinition(StrictBaseModel):
    """Tool definition exposed to the model."""

    type: Literal["function"] = "function"
    function: FunctionSpec


def _empty_tools() -> list[ToolDefinition]:
    return []


class TextContent(StrictBaseModel):
    """Text emitted by the model."""

    type: Literal["text"] = "text"
    text: str = Field(min_length=1)


class ReasoningContent(StrictBaseModel):
    """Reasoning summary emitted by the model when a provider returns one."""

    type: Literal["reasoning"] = "reasoning"
    text: str = Field(min_length=1)


class ToolUseContent(StrictBaseModel):
    """Tool call emitted by the model after backend normalization."""

    type: Literal["tool_use"] = "tool_use"
    id: str = Field(min_length=1, max_length=128)
    name: str = Field(pattern=r"^[A-Za-z0-9_.-]+$", max_length=128)
    input: dict[str, object] = Field(default_factory=dict)


ContentBlock = Annotated[
    TextContent | ReasoningContent | ToolUseContent,
    Field(discriminator="type"),
]


class Usage(StrictBaseModel):
    """Token usage for one backend response."""

    input_tokens: int = Field(ge=0)
    output_tokens: int = Field(ge=0)
    total_tokens: int = Field(ge=0)


class MaestroRequest(StrictBaseModel):
    """Provider-neutral request passed into `MaestroBackend.respond`."""

    messages: list[Message] = Field(min_length=1)
    tools: list[ToolDefinition] = Field(default_factory=_empty_tools)
    reasoning_effort: ReasoningEffort = "medium"
    budget_usd: float = Field(gt=0)
    trace_id: TraceId
    parent_trace_id: TraceId | None = None
    max_input_tokens: int | None = Field(default=None, gt=0)


class MaestroResponse(StrictBaseModel):
    """Provider-neutral response returned by `MaestroBackend.respond`."""

    content: list[ContentBlock] = Field(min_length=1)
    stop_reason: StopReason
    usage: Usage
    cost_usd: float = Field(ge=0)
