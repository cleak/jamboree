---
id: insight-deterministic-then-llm-pattern
type: insight
created: 2026-05-04T03:48:16.587921850Z
updated: 2026-05-04T05:05:51.825670925Z
edges:
- target: comp-patch-agent
  type: relates_to
- target: feat-hot-patching
  type: informs
---
**Deterministic-then-LLM** is a recurring pattern (§20.5 patch agent; cited as design model).

For any "recover from failure" workflow:
1. Cheap deterministic checks first (~80% of cases).
2. Mechanical recovery (rollback, restart, retry).
3. Only if both fail: LLM diagnosis with bounded cost.
4. If LLM also fails: incident dump + ntfy human + halt.

Deterministic catches the common cases at near-zero cost. LLM handles novelty. Hard halt on exhaustion is safer than runaway recovery loops.

Applies similarly to: stall detection → Maestro reasoning → human escalation; quota exhaustion → fall-through tier → ntfy; search backend cooldown → fallback chain → surface error.