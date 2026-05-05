---
id: task-mechanical-rollback-flow
type: task
status: backlog
created: 2026-05-04T04:00:27.135822409Z
updated: 2026-05-04T04:15:50.289070857Z
edges:
- target: feat-hot-patching
  type: child_of
---
Phase 7 (§12). Mechanical rollback flow.

Per `comp-rollback-flow`.

Old service stays alive in swap window; if health checks fail, point manifest back at it.