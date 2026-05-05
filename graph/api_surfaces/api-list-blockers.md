---
id: api-list-blockers
type: api_surface
status: draft
created: 2026-05-04T03:51:51.168556162Z
updated: 2026-05-04T04:53:18.066781038Z
edges:
- target: comp-jam-svc-observe
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`list-blockers(task-id)` → `Vec<Blocker>` (§5.1). Returns blockers when `compute-readiness` is `NotReady`.