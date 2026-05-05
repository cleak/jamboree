---
id: metric-patch-agent-llm-budget
type: metric
status: proposed
created: 2026-05-04T03:48:06.559388406Z
updated: 2026-05-04T03:48:06.559389003Z
---
**Patch agent LLM diagnosis budget**: $0.50 single-turn (§20.5 step C).

Cost cap on the focused LLM session that runs only when deterministic recovery + mechanical rollback have failed. Default model: Claude Haiku 4.5 or GPT-5.5-mini.

If exceeded or LLM diagnosis fails: write incident dump to `~/.jam/incidents/<id>/`, ntfy critical, pause-dispatch, patch agent process exits.