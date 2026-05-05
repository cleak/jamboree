---
id: dec-litellm-for-maestro
type: decision
status: decided
created: 2026-05-04T03:46:12.479489152Z
updated: 2026-05-04T05:01:40.103386387Z
edges:
- target: comp-litellm-backend
  type: decision_for
- target: feat-maestro-orchestration-loop
  type: depended_on_by
---
**LiteLLM as Maestro provider abstraction** (§4.1, §19.1). The Maestro never directly imports `openai` or `anthropic`. All LLM calls go through `MaestroBackend` (default `LiteLLMBackend`).

Why: LiteLLM presents a uniform interface across ~100 providers. Single config flip points the Maestro at Claude (via API), Gemini 3.5, OpenRouter, Hermes-from-Nous-Portal, or any other provider. When policy weather hits (April 4 2026 Anthropic block being canonical), Maestro keeps working.

Per memory: Maestro uses ChatGPT subscription OAuth (not API key); try LiteLLM `chatgpt/*` provider first, fall back to custom `MaestroBackend` impl wrapping `codex-auth`. GPT-5.5 required.