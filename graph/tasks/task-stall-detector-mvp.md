---
id: task-stall-detector-mvp
type: task
status: done
created: 2026-05-04T03:58:38.537667996Z
updated: 2026-05-06T05:39:03Z
edges:
- target: feat-failure-handling
  type: child_of
---
Phase 1 (§12). Stall detector with token-idle and tool-loop rules.

Per `comp-stall-detector`, `metric-stall-token-idle-secs`.

Acceptance: a Picker that emits no tokens for 90s emits `picker.stalled`; a Picker that calls the same tool 3+ times in a row emits `picker.stalled`.

Implementation note (2026-05-06): `crates/jam-stall-detector` now provides bin `jam-stall-detector`. It subscribes to traced `picker.*.lifecycle` and `picker.*.output`, tracks per-session last output and repeated tool signatures, and emits traced `picker.stalled` for `token-idle` and `tool-loop` without taking policy action. Unit tests cover both rules; a live temporary-NATS smoke verified token-idle and repeated-tool messages both produce `picker.stalled`.
