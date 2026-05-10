---
id: api-world-snapshot-delta
type: api_surface
status: stable
created: 2026-05-04T03:51:44.945274085Z
updated: 2026-05-06T21:14:48Z
edges:
- target: comp-jam-svc-observe
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`world-snapshot-delta(task-id, since)` → only fields that changed since `since` (§5.1, §4.1.3 mitigation B).

Cheap input-budget mitigation: Maestro often wakes for a task it last worked on minutes ago. Full snapshot is expensive in context tokens; delta is cheap.

Per-Maestro-instance "last seen" cursor stored in substrate.

Implementation note (2026-05-06): active in `jam-svc-observe` as a
conservative cache-backed delta. If the service has a cached baseline that is
not newer than the caller's `since`, it returns only changed snapshot fields.
If no safe baseline exists, it returns `full=true` with the full snapshot fields
and a reason, so the Maestro never misses changes while reducing context when
the cache baseline is usable.
