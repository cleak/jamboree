---
id: feat-input-budget-management
type: feature
status: draft
created: 2026-05-04T03:28:15.745817361Z
updated: 2026-05-04T04:06:57.822452410Z
owner: caleb
edges:
- target: comp-maestro-session-loop
  type: uses
- target: jamboree-v5
  type: child_of
- target: principle-episodic-maestro-sessions
  type: constrained_by
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: the-manager
  type: serves
---
Three-mitigation stack for session-start input cost (§4.1.3):

A. **Relevance-scoped skill loading.** `read-skills(scope)` returns only skills matching the wake's scope. Scope is hierarchical (e.g., `blueberry/coderabbit-review/canyon-area`). Maybe 8–15 skills loaded, not 50+.

B. **Delta snapshots.** First call on a known task uses `world-snapshot-delta(task_id, since=last_seen_for(task_id))`; falls through to full snapshot only if delta is substantial. Per-Maestro-instance "last seen" cursor stored in substrate.

C. **Explicit input budgets.** `~/.jam/config/maestro.toml` declares per-session-input-tokens, per-session-output-tokens, daily-usd, and input-budget caps for skill files / journal replay / world-snapshot. Loader assembles within budget, prioritizing wake context > world-snapshot > scoped skills > journal events.

Implementation note (2026-05-06): explicit input budget assembly is active in the Python Maestro scaffold via `jam_maestro.input_budget`. Delta snapshots remain future work, but the configured budget caps and session-loop reporting are implemented for full snapshots, scoped skills, and journal replay inputs.
