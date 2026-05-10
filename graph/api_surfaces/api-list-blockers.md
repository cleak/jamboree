---
id: api-list-blockers
type: api_surface
status: stable
created: 2026-05-04T03:51:51.168556162Z
updated: 2026-05-06T21:18:32Z
edges:
- target: comp-jam-svc-observe
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`list-blockers(task-id)` → `Vec<Blocker>` (§5.1). Returns blockers when `compute-readiness` is `NotReady`.

Implementation note (2026-05-06): `tool.observe.list-blockers` is implemented in `jam-svc-observe` and returns the blocker list computed from the same world snapshot used for readiness.
