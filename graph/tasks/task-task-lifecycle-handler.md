---
id: task-task-lifecycle-handler
type: task
status: done
created: 2026-05-04T03:58:33.201300198Z
updated: 2026-05-06T05:18:05Z
edges:
- target: feat-task-tracking-via-lifecycle-transitions
  type: child_of
---
Phase 1 (§12). Tempyr task node lifecycle handler (`jam-task-lifecycle`) — writes `tempyr/tasks/<id>.yaml` on lifecycle transitions.

Per `comp-task-lifecycle-handler`, `feat-task-tracking-via-lifecycle-transitions`.

Acceptance: on `picker.spawned`, task node appears in `tempyr/tasks/` with status=in-progress; on `pr.merged`, status=merged with merged-sha + outcome.

Implementation note (2026-05-06): `crates/jam-task-lifecycle` now subscribes to traced `journal.>` events, handles `picker.spawned`, `pr.opened`, `pr.merged`, and `task.abandoned`, writes Markdown task nodes under the configured canonical Tempyr worktree (`JAM_CANONICAL_TEMPYR_WORKTREE` / `JAM_TEMPYR_WORKTREE` plus `JAM_GRAPH_RELPATH`), and emits `journal.tempyr.task-updated`. Unit tests cover spawn, PR-open, and merge transitions. Live smoke with temporary NATS verified spawn -> PR-open updates to `status: in-review`; a second traced publish smoke verified `pr.merged` updates to `status: merged`, records `merged-sha`, sets `outcome: merged`, and journals `tempyr.task-updated`.
