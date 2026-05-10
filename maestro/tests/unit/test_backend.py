from __future__ import annotations

import pytest

from jam_maestro.backend import LiteLLMBackend, MaestroBackendError
from jam_maestro.models import FunctionSpec, MaestroRequest, Message, ToolDefinition

TRACE_ID = "01HXKJ00000000000000000000"
EXPECTED_TOTAL_TOKENS = 15
EXPECTED_COST_USD = 0.00042


def test_litellm_backend_normalizes_text_response() -> None:
    calls: list[dict[str, object]] = []

    def completion(**kwargs: object) -> object:
        calls.append(kwargs)
        return {
            "choices": [
                {
                    "message": {"content": "pong"},
                    "finish_reason": "stop",
                }
            ],
            "usage": {
                "prompt_tokens": 12,
                "completion_tokens": 3,
                "total_tokens": 15,
            },
            "response_cost": 0.00042,
        }

    backend = LiteLLMBackend(model="chatgpt/gpt-5.5", completion=completion)
    response = backend.respond(
        MaestroRequest(
            messages=[Message(role="user", content="ping")],
            reasoning_effort="medium",
            budget_usd=0.25,
            trace_id=TRACE_ID,
        )
    )

    assert response.content[0].type == "text"
    assert response.content[0].text == "pong"
    assert response.stop_reason == "end_turn"
    assert response.usage.total_tokens == EXPECTED_TOTAL_TOKENS
    assert response.cost_usd == EXPECTED_COST_USD

    assert calls[0]["model"] == "chatgpt/gpt-5.5"
    assert calls[0]["messages"] == [{"role": "user", "content": "ping"}]
    assert calls[0]["reasoning"] == {"effort": "medium"}
    assert calls[0]["metadata"] == {
        "trace_id": TRACE_ID,
        "parent_trace_id": None,
    }


def test_litellm_backend_normalizes_tool_calls() -> None:
    def completion(**_kwargs: object) -> object:
        return {
            "choices": [
                {
                    "message": {
                        "content": "",
                        "tool_calls": [
                            {
                                "id": "call-1",
                                "function": {
                                    "name": "world-snapshot",
                                    "arguments": '{"task_id":"task-1"}',
                                },
                            }
                        ],
                    },
                    "finish_reason": "tool_calls",
                }
            ],
            "usage": {
                "input_tokens": 20,
                "output_tokens": 5,
                "total_tokens": 25,
            },
            "_hidden_params": {"response_cost": 0.001},
        }

    backend = LiteLLMBackend(model="gpt-5.5", completion=completion)
    response = backend.respond(
        MaestroRequest(
            messages=[Message(role="user", content="snapshot")],
            tools=[
                ToolDefinition(
                    function=FunctionSpec(
                        name="world-snapshot",
                        description="Compile current task truth.",
                    )
                )
            ],
            trace_id=TRACE_ID,
            budget_usd=0.25,
        )
    )

    block = response.content[0]
    assert block.type == "tool_use"
    assert block.name == "world-snapshot"
    assert block.input == {"task_id": "task-1"}
    assert response.stop_reason == "tool_use"


def test_litellm_backend_requires_cost_by_default() -> None:
    def completion(**_kwargs: object) -> object:
        return {
            "choices": [
                {
                    "message": {"content": "pong"},
                    "finish_reason": "stop",
                }
            ],
            "usage": {
                "prompt_tokens": 1,
                "completion_tokens": 1,
                "total_tokens": 2,
            },
        }

    backend = LiteLLMBackend(
        model="custom/no-pricing",
        completion=completion,
        cost_calculator=lambda _raw: (_ for _ in ()).throw(RuntimeError("no price")),
    )

    with pytest.raises(MaestroBackendError, match="cost_usd"):
        backend.respond(
            MaestroRequest(
                messages=[Message(role="user", content="ping")],
                trace_id=TRACE_ID,
                budget_usd=0.25,
            )
        )
