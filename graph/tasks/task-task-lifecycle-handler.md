---
id: task-task-lifecycle-handler
type: task
status: backlog
created: 2026-05-04T03:58:33.201300198Z
updated: 2026-05-04T04:10:26.317055201Z
edges:
- target: feat-task-tracking-via-lifecycle-transitions
  type: child_of
---
Phase 1 (§12). Tempyr task node lifecycle handler (`jam-task-lifecycle`) — writes `tempyr/tasks/<id>.yaml` on lifecycle transitions.

Per `comp-task-lifecycle-handler`, `feat-task-tracking-via-lifecycle-transitions`.

Acceptance: on `picker.spawned`, task node appears in `tempyr/tasks/` with status=in-progress; on `pr.merged`, status=merged with merged-sha + outcome.