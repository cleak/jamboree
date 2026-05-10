---
id: task-maestro-session-loop
type: task
status: done
created: 2026-05-04T03:58:26.020997934Z
updated: 2026-05-06T04:22:45.483229738Z
edges:
- target: feat-maestro-orchestration-loop
  type: child_of
---
Phase 1 (§12). Maestro session loop: wake on bus event, load skills, call `world-snapshot`, decide, call tools, close.

Per `comp-maestro-session-loop`, `comp-maestro-wake-handler`.

Acceptance: Maestro wakes on `journal.task.requested`, loads skills with scope, calls `world-snapshot`, emits at least one `decision` entry into the Tempyr journal, closes session cleanly.