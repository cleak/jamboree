---
id: risk-maestro-token-cost
type: risk
status: identified
created: 2026-05-04T03:46:57.277763666Z
updated: 2026-05-04T03:46:57.277764182Z
---
**§13.5 Maestro token cost.** GPT-5.5 with high reasoning is expensive ($X per session, where X depends on session length and reasoning effort).

Mitigation: per-session and daily budgets make this bounded; budgets are configurable. If budgets are systematically hit, the Maestro model can be swapped for a cheaper model (DeepSeek V4 Pro, Sonnet) via the LiteLLM backend.