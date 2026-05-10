"""Provider-agnostic Maestro LLM backend protocol and LiteLLM implementation."""

from __future__ import annotations

import importlib
import json
from collections.abc import Callable, Mapping, Sequence
from typing import NoReturn, Protocol, cast

from jam_maestro.models import (
    ContentBlock,
    MaestroRequest,
    MaestroResponse,
    ReasoningContent,
    StopReason,
    TextContent,
    ToolUseContent,
    Usage,
)


class MaestroBackendError(RuntimeError):
    """Raised when a backend response cannot satisfy the Maestro contract."""


class MaestroBackend(Protocol):
    """Protocol all Maestro model providers implement."""

    def respond(self, req: MaestroRequest) -> MaestroResponse:
        """Return a normalized Maestro response for one episodic request."""
        ...


CompletionCallable = Callable[..., object]
CostCalculator = Callable[[object], float]
DynamicCallable = Callable[..., object]


class LiteLLMBackend:
    """LiteLLM-backed implementation of `MaestroBackend`.

    Per §2.8 and `dec-litellm-for-maestro`, the Maestro talks to provider-neutral
    LiteLLM shapes here and does not import provider SDKs directly.
    """

    def __init__(
        self,
        model: str,
        *,
        completion: CompletionCallable | None = None,
        cost_calculator: CostCalculator | None = None,
        allow_unknown_cost: bool = False,
        **completion_kwargs: object,
    ) -> None:
        self._model = model
        self._completion = completion or _default_completion
        self._cost_calculator = cost_calculator or _default_cost_calculator
        self._allow_unknown_cost = allow_unknown_cost
        self._completion_kwargs = completion_kwargs

    @property
    def model(self) -> str:
        """Configured LiteLLM model identifier."""
        return self._model

    def respond(self, req: MaestroRequest) -> MaestroResponse:
        """Call LiteLLM and normalize the result into `MaestroResponse`."""
        raw = self._completion(**self._request_kwargs(req))
        envelope = _as_mapping(raw, "completion response")
        choice = _first_choice(envelope)
        message = _as_mapping(_required(choice, "message"), "choices[0].message")

        content = _content_blocks(message)
        usage = _usage(_as_mapping(_required(envelope, "usage"), "usage"))
        cost_usd = self._cost_usd(envelope, raw)

        return MaestroResponse(
            content=content,
            stop_reason=_stop_reason(choice),
            usage=usage,
            cost_usd=cost_usd,
        )

    def _request_kwargs(self, req: MaestroRequest) -> dict[str, object]:
        kwargs: dict[str, object] = {
            "model": self._model,
            "messages": [
                message.model_dump(mode="json", exclude_none=True) for message in req.messages
            ],
            "metadata": {
                "trace_id": req.trace_id,
                "parent_trace_id": req.parent_trace_id,
            },
            "reasoning": {"effort": req.reasoning_effort},
        }
        if req.tools:
            kwargs["tools"] = [
                tool.model_dump(mode="json", exclude_none=True) for tool in req.tools
            ]
        if req.max_input_tokens is not None:
            kwargs["metadata"] = {
                **cast("dict[str, object]", kwargs["metadata"]),
                "max_input_tokens": req.max_input_tokens,
            }
        kwargs.update(self._completion_kwargs)
        return kwargs

    def _cost_usd(self, envelope: Mapping[str, object], raw: object) -> float:
        cost = _optional_float(envelope, "response_cost")
        if cost is not None:
            return cost

        hidden = envelope.get("_hidden_params")
        if hidden is not None:
            hidden_mapping = _as_mapping(hidden, "_hidden_params")
            cost = _optional_float(hidden_mapping, "response_cost")
            if cost is not None:
                return cost

        try:
            return self._cost_calculator(raw)
        except Exception as exc:
            if self._allow_unknown_cost:
                return 0.0
            message = "LiteLLM response did not include computable cost_usd"
            raise MaestroBackendError(message) from exc


def _default_completion(**kwargs: object) -> object:
    module = importlib.import_module("litellm")
    completion = cast(
        "CompletionCallable",
        vars(module)["completion"],
    )
    return completion(**kwargs)


def _default_cost_calculator(raw: object) -> float:
    module = importlib.import_module("litellm")
    completion_cost = cast(
        "DynamicCallable",
        vars(module)["completion_cost"],
    )
    cost = completion_cost(completion_response=raw)
    if isinstance(cost, bool) or not isinstance(cost, int | float):
        _raise_backend_error("LiteLLM completion_cost returned a non-numeric value")
    return float(cost)


def _first_choice(envelope: Mapping[str, object]) -> Mapping[str, object]:
    choices = _as_sequence(_required(envelope, "choices"), "choices")
    if not choices:
        _raise_backend_error("LiteLLM response had no choices")
    return _as_mapping(choices[0], "choices[0]")


def _content_blocks(message: Mapping[str, object]) -> list[ContentBlock]:
    blocks: list[ContentBlock] = []
    content = message.get("content")
    if isinstance(content, str) and content:
        blocks.append(TextContent(text=content))
    elif content is not None and not isinstance(content, str):
        blocks.extend(_structured_content_blocks(content))

    reasoning = message.get("reasoning_content") or message.get("reasoning")
    if isinstance(reasoning, str) and reasoning:
        blocks.append(ReasoningContent(text=reasoning))

    tool_calls = message.get("tool_calls")
    if tool_calls is not None:
        blocks.extend(_tool_use_blocks(tool_calls))

    if not blocks:
        _raise_backend_error("LiteLLM response message contained no content blocks")
    return blocks


def _structured_content_blocks(content: object) -> list[ContentBlock]:
    blocks: list[ContentBlock] = []
    for index, item in enumerate(_as_sequence(content, "message.content")):
        item_map = _as_mapping(item, f"message.content[{index}]")
        kind = item_map.get("type")
        if kind == "text":
            blocks.append(TextContent(text=_required_str(item_map, "text")))
        elif kind == "reasoning":
            blocks.append(ReasoningContent(text=_required_str(item_map, "text")))
        else:
            _raise_backend_error(f"unsupported content block type: {kind!r}")
    return blocks


def _tool_use_blocks(tool_calls: object) -> list[ContentBlock]:
    blocks: list[ContentBlock] = []
    for index, item in enumerate(_as_sequence(tool_calls, "message.tool_calls")):
        item_map = _as_mapping(item, f"message.tool_calls[{index}]")
        function = _as_mapping(_required(item_map, "function"), "tool_call.function")
        blocks.append(
            ToolUseContent(
                id=_required_str(item_map, "id"),
                name=_required_str(function, "name"),
                input=_tool_arguments(function.get("arguments")),
            )
        )
    return blocks


def _tool_arguments(arguments: object) -> dict[str, object]:
    if arguments is None:
        return {}
    if isinstance(arguments, str):
        parsed = json.loads(arguments)
        if not isinstance(parsed, dict):
            _raise_backend_error("tool call arguments must decode to an object")
        return cast("dict[str, object]", parsed)
    if isinstance(arguments, dict):
        return cast("dict[str, object]", arguments)
    return _raise_backend_error("tool call arguments must be a JSON string or object")


def _usage(raw: Mapping[str, object]) -> Usage:
    input_tokens = _optional_int(raw, "prompt_tokens")
    if input_tokens is None:
        input_tokens = _optional_int(raw, "input_tokens")
    output_tokens = _optional_int(raw, "completion_tokens")
    if output_tokens is None:
        output_tokens = _optional_int(raw, "output_tokens")
    if input_tokens is None or output_tokens is None:
        _raise_backend_error("LiteLLM usage must include input and output token counts")
    total_tokens = _optional_int(raw, "total_tokens") or input_tokens + output_tokens
    return Usage(
        input_tokens=input_tokens,
        output_tokens=output_tokens,
        total_tokens=total_tokens,
    )


def _stop_reason(choice: Mapping[str, object]) -> StopReason:
    raw = choice.get("finish_reason")
    if raw == "stop":
        return "end_turn"
    if raw == "tool_calls":
        return "tool_use"
    if raw == "length":
        return "max_tokens"
    if raw == "content_filter":
        return "content_filter"
    return "other"


def _required(mapping: Mapping[str, object], key: str) -> object:
    try:
        return mapping[key]
    except KeyError:
        _raise_backend_error(f"LiteLLM response missing {key}")


def _required_str(mapping: Mapping[str, object], key: str) -> str:
    value = _required(mapping, key)
    if not isinstance(value, str) or not value:
        _raise_backend_error(f"{key} must be a non-empty string")
    return value


def _optional_int(mapping: Mapping[str, object], key: str) -> int | None:
    value = mapping.get(key)
    if value is None:
        return None
    if isinstance(value, bool) or not isinstance(value, int):
        _raise_backend_error(f"{key} must be an integer")
    return value


def _optional_float(mapping: Mapping[str, object], key: str) -> float | None:
    value = mapping.get(key)
    if value is None:
        return None
    if isinstance(value, bool) or not isinstance(value, int | float):
        _raise_backend_error(f"{key} must be numeric")
    return float(value)


def _as_mapping(value: object, name: str) -> Mapping[str, object]:
    if isinstance(value, Mapping):
        return cast("Mapping[str, object]", value)
    dump = getattr(value, "model_dump", None)
    if callable(dump):
        dumped = dump()
        if isinstance(dumped, Mapping):
            return cast("Mapping[str, object]", dumped)
    return _raise_backend_error(f"{name} must be an object")


def _as_sequence(value: object, name: str) -> Sequence[object]:
    if isinstance(value, str | bytes):
        _raise_backend_error(f"{name} must be an array")
    if isinstance(value, Sequence):
        return cast("Sequence[object]", value)
    return _raise_backend_error(f"{name} must be an array")


def _raise_backend_error(message: str) -> NoReturn:
    raise MaestroBackendError(message)
