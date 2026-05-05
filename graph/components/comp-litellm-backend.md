---
id: comp-litellm-backend
type: component
status: planned
created: 2026-05-04T03:31:28.851727765Z
updated: 2026-05-04T05:01:49.472112339Z
edges:
- target: api-maestro-backend-protocol
  type: exposes
- target: comp-maestro-process
  type: depended_on_by
- target: dec-chatgpt-subscription-oauth-for-maestro
  type: has_decision
- target: dec-litellm-for-maestro
  type: has_decision
- target: feat-maestro-orchestration-loop
  type: used_by
- target: principle-episodic-maestro-sessions
  type: constrained_by
- target: principle-provider-agnostic-everywhere
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
---
`MaestroBackend` protocol with `LiteLLMBackend` default impl (§4.1, §19.1). The Maestro never directly imports `openai` or `anthropic`; all LLM calls go through `MaestroBackend.respond(MaestroRequest) -> MaestroResponse`.

Provider-agnostic plumbing per §2.8 — when policy weather hits, swapping is `model = "gpt-5.5"` → `model = "claude-sonnet-4-6"` config change, no code changes.

`MaestroRequest`: messages, tools, reasoning_effort, budget_usd, trace_id, parent_trace_id, max_input_tokens.
`MaestroResponse`: content blocks, stop_reason, usage, cost_usd.

Per memory: ChatGPT subscription OAuth path is non-standard — try LiteLLM's `chatgpt/*` provider first; if unavailable, write a custom `MaestroBackend` impl that wraps `codex-auth` for the Codex OAuth credential.