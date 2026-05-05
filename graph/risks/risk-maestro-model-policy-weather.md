---
id: risk-maestro-model-policy-weather
type: risk
status: identified
created: 2026-05-04T03:47:09.211239016Z
updated: 2026-05-04T03:47:09.211239955Z
---
**§13.12 Maestro model policy weather.** The April 4 2026 Anthropic block is the canonical example. Future provider policy shifts could affect any model we depend on.

Mitigation: LiteLLM abstraction means swapping is config-only; the orchestrator runs on any provider that LiteLLM supports.

This is the why behind `principle-provider-agnostic-everywhere`.