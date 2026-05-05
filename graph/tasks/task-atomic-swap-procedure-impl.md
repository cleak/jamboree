---
id: task-atomic-swap-procedure-impl
type: task
status: backlog
created: 2026-05-04T04:00:15.860123654Z
updated: 2026-05-04T04:15:15.460339269Z
edges:
- target: feat-hot-patching
  type: child_of
---
Phase 7 (§12). Atomic-swap procedure for tool services.

Per `comp-atomic-swap-procedure`.

Acceptance: patch a tool service while the Maestro is mid-session; verify in-flight calls complete, new calls hit the new version, no session interruption.