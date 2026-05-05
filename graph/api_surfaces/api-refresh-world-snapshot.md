---
id: api-refresh-world-snapshot
type: api_surface
status: draft
created: 2026-05-04T03:51:47.016502883Z
updated: 2026-05-04T04:53:00.016167893Z
edges:
- target: comp-jam-svc-observe
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`refresh-world-snapshot(task-id)` → forces refetch (§5.1, §4.2.1). Bypasses TTL/event-driven cache.

Used when Maestro suspects staleness despite freshness tags claiming otherwise.