---
id: api-world-snapshot
type: api_surface
status: draft
created: 2026-05-04T03:51:42.842768496Z
updated: 2026-05-04T04:52:42.915011383Z
edges:
- target: comp-jam-svc-observe
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`world-snapshot(task-id-or-pr-url, max-staleness-secs?)` → `WorldSnapshot` (§5.1, §4.2.1).

The fact compiler. Every Maestro decision starts here per `principle-observable-not-deterministic`. Returns session, worktree, branch_staleness, PR, CI, review_artifacts, blockers, readiness, harness_quotas, tempyr_index_cursor, recent_dead_ends — plus per-source `freshness` map.

Cached with event-driven invalidation backed by 60s TTL.