---
id: task-litellm-backend-skeleton
type: task
status: done
created: 2026-05-04T03:58:10.184627090Z
updated: 2026-05-06T03:39:31.764400272Z
edges:
- target: feat-maestro-orchestration-loop
  type: child_of
---
Phase 0 (§12). Maestro backend skeleton (`LiteLLMBackend`) — can make a dummy LLM call; no tool surface yet.

Per `comp-litellm-backend`, `dec-litellm-for-maestro`. Per memory: try LiteLLM `chatgpt/*` for ChatGPT subscription OAuth path first; if unavailable, write a custom `MaestroBackend` impl wrapping `codex-auth`.

Acceptance: `MaestroBackend::respond(MaestroRequest)` returns a `MaestroResponse` with cost_usd, usage, content blocks for a trivial test prompt.