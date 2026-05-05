---
id: api-pr-status
type: api_surface
status: draft
created: 2026-05-04T03:52:20.626986789Z
updated: 2026-05-04T04:55:12.536207822Z
edges:
- target: comp-jam-svc-repo
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`pr-status(pr-ref)` → typed PR state (§5.4). Uses ETag-conditional GitHub request.