---
id: task-jam-svc-observe-mvp
type: task
status: backlog
created: 2026-05-04T03:58:15.402076152Z
updated: 2026-05-04T04:09:34.266171054Z
edges:
- target: feat-observation-tool-service
  type: child_of
---
Phase 1 (§12). `jam-svc-observe` implementing `world-snapshot`, `compute-readiness`, `list-blockers`, `branch-staleness`. Cache layer with both event-driven invalidation and 60s TTL.

Per `comp-jam-svc-observe`, `comp-world-snapshot`, `comp-world-snapshot-cache`, `comp-compute-readiness`, `comp-branch-staleness`.

Acceptance: Maestro calls `world-snapshot` via NATS, gets a typed response with `freshness` map; second call within 60s returns cached.