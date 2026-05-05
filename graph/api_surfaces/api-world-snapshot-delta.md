---
id: api-world-snapshot-delta
type: api_surface
status: draft
created: 2026-05-04T03:51:44.945274085Z
updated: 2026-05-04T04:52:51.965462385Z
edges:
- target: comp-jam-svc-observe
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`world-snapshot-delta(task-id, since)` → only fields that changed since `since` (§5.1, §4.1.3 mitigation B).

Cheap input-budget mitigation: Maestro often wakes for a task it last worked on minutes ago. Full snapshot is expensive in context tokens; delta is cheap.

Per-Maestro-instance "last seen" cursor stored in substrate.