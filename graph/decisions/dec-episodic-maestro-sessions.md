---
id: dec-episodic-maestro-sessions
type: decision
status: decided
created: 2026-05-04T03:46:32.441786380Z
updated: 2026-05-04T05:05:04.245007638Z
edges:
- target: comp-maestro-process
  type: decision_for
- target: feat-maestro-orchestration-loop
  type: depended_on_by
---
**Episodic Maestro sessions, not a persistent loop** (§4.1.2). Each session is a single LLM conversation, opened on wake, closed on done/budget/interrupt/fatal. Persistent state lives in skills, journal, Tempyr, user-edits memory.

Why: a persistent agent loop has compounding context drift, debugging the cause of a misbehaving turn becomes harder over hours, and token cost grows quadratically. Episodic sessions cap each cost.

A consequence: there is no `fork-Maestro` or `clone-session` tool (§5.9).

Captured as `principle-episodic-maestro-sessions`.