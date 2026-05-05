---
id: comp-maestro-process
type: component
status: planned
created: 2026-05-04T03:31:28.326090170Z
updated: 2026-05-04T05:05:04.245007184Z
edges:
- target: comp-litellm-backend
  type: depends_on
- target: dec-episodic-maestro-sessions
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
The Maestro: long-running Python process running episodic LLM sessions (§4.1). Default model `gpt-5.5` via OpenAI Responses API; `gpt-5.5-pro` for hard reasoning passes.

Reasoning effort: `medium` for routine, `high` for review-pass scoring, `xhigh` for hard cases. Reasoning tokens count against output billing — budgeted explicitly because `xhigh` calls can hit 20K reasoning tokens on long prompts.

Process is idle between sessions, awaiting wake events. Wake → load skills + world-snapshot + budget → reason via tool calls → close.

Per memory: Maestro uses ChatGPT subscription OAuth (not API key) — try LiteLLM `chatgpt/*` first, fall back to custom provider wrapping `codex-auth`. GPT-5.5 required.

Lives in `maestro/src/jam_maestro/` per §11.1.