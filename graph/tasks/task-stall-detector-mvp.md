---
id: task-stall-detector-mvp
type: task
status: backlog
created: 2026-05-04T03:58:38.537667996Z
updated: 2026-05-04T04:10:40.535561247Z
edges:
- target: feat-failure-handling
  type: child_of
---
Phase 1 (§12). Stall detector with token-idle and tool-loop rules.

Per `comp-stall-detector`, `metric-stall-token-idle-secs`.

Acceptance: a Picker that emits no tokens for 90s emits `picker.stalled`; a Picker that calls the same tool 3+ times in a row emits `picker.stalled`.