---
id: principle-episodic-maestro-sessions
type: constraint
status: active
created: 2026-05-04T03:23:50.218642390Z
updated: 2026-05-04T04:17:43.624160775Z
edges:
- target: comp-litellm-backend
  type: constrains
- target: comp-maestro-process
  type: constrains
- target: comp-maestro-session-loop
  type: constrains
- target: feat-budget-enforcement
  type: constrains
- target: feat-input-budget-management
  type: constrains
- target: feat-maestro-orchestration-loop
  type: constrains
---
**Episodic Maestro sessions, not a persistent loop.**

Each Maestro session is a single LLM conversation, opened on wake, closed when the Maestro emits a "done" output, hits a budget cap, gets interrupted, or hits a fatal error. After session close, context is discarded; persistent state lives in skills, journal, Tempyr, and the user-edits memory.

*Why episodic (§4.1.2):* a persistent agent loop has compounding context drift, debugging the cause of a misbehaving turn becomes harder over hours, and token cost grows quadratically. Episodic sessions cap each cost. The Maestro is stateful between sessions only via durable artifacts.

A consequence: there is no `fork-Maestro` or `clone-session` tool (§5.9).