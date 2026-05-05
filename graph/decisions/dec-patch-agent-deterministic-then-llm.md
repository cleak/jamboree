---
id: dec-patch-agent-deterministic-then-llm
type: decision
status: decided
created: 2026-05-04T03:46:06.849931025Z
updated: 2026-05-04T05:01:12.477068887Z
edges:
- target: comp-patch-agent
  type: decision_for
- target: feat-hot-patching
  type: depended_on_by
---
**Patch agent with deterministic-then-LLM recovery** (§v5 changes #3, §20.5).

Procedure: A. Deterministic checks (cheap, ~80% of failures) → B. If A fails, mechanical rollback → C. LLM diagnosis ($0.50 budget cap, single-turn) only if mechanical rollback insufficient → D. Incident dump + ntfy critical if all else fails.

Why: deterministic checks are near-zero cost and catch most patch failures. LLM only kicks in for the harder cases where structured failure data needs interpretation. If deterministic recovery works, no LLM cost is incurred.

Hot-patching without supervision creates silent breakage. Failed recoveries that auto-rollback are recoverable; failures that hang are not.