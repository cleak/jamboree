---
id: task-trace-replay-tool-prove
type: task
status: backlog
created: 2026-05-04T03:58:43.978091789Z
updated: 2026-05-04T04:10:55.412110622Z
edges:
- target: feat-trace-propagation
  type: child_of
---
Phase 1 (§12). `trace-replay` tool — proves the trace chain works end-to-end.

Per `comp-trace-replay-tool`, `api-trace-replay`.

Acceptance: trace from Picker spawn back to Maestro wake reconstructible via `trace-replay`. Kill the Picker mid-session (`full-stop`); verify worktree is preserved with `.killed-at-` marker, Tempyr task node updated to `abandoned`, journal session finalized cleanly.