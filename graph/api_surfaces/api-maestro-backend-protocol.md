---
id: api-maestro-backend-protocol
type: api_surface
status: stable
created: 2026-05-04T03:53:43.687815414Z
updated: 2026-05-06T21:29:02Z
edges:
- target: comp-litellm-backend
  type: exposed_by
- target: feat-maestro-orchestration-loop
  type: exposed_by
---
The `MaestroBackend` Python protocol (§4.1, §19.1):

```python
class MaestroBackend(Protocol):
    def respond(self, req: MaestroRequest) -> MaestroResponse: ...
```

Default `LiteLLMBackend` impl. Custom implementations may wrap subscription auth (e.g., ChatGPT Pro via `codex-auth` per memory).

`MaestroRequest`: messages, tools, reasoning_effort, budget_usd, trace_id, parent_trace_id, max_input_tokens.
`MaestroResponse`: content blocks, stop_reason, usage, cost_usd.

Implementation note (2026-05-06): `maestro/src/jam_maestro/backend.py` defines the `MaestroBackend` protocol and `LiteLLMBackend` implementation with normalized content blocks, usage, stop reason, and required cost accounting.
