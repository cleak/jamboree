---
id: dec-chatgpt-subscription-oauth-for-maestro
type: decision
status: decided
created: 2026-05-04T03:46:13.901802558Z
updated: 2026-05-04T05:01:49.472112732Z
edges:
- target: comp-litellm-backend
  type: decision_for
- target: feat-maestro-orchestration-loop
  type: depended_on_by
---
**Maestro auth: ChatGPT subscription OAuth (not API key)** (security-setup §5.3, memory).

Per memory: try LiteLLM `chatgpt/*` provider first; fall back to custom provider wrapping `codex-auth`. GPT-5.5 is the required model.

Manual setup: `codex login --device-auth` while logged in as `maestro` user — required because maestro has no local browser session for default OAuth redirect. Token at `~maestro/.codex/auth.json` (mode 600). Same Codex OAuth credential powers any Codex-based Picker, so no separate harness token needed.

Implication for `pass`: GPT-5.5 is subscription-gated; no `jam/maestro/openai-api-key` entry needed unless switching to a non-Codex provider.