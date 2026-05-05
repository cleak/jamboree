---
id: api-mark-review-artifact-handled
type: api_surface
status: draft
created: 2026-05-04T03:52:27.306509995Z
updated: 2026-05-04T04:55:39.062897022Z
edges:
- target: comp-jam-svc-repo
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`mark-review-artifact-handled(artifact-id, status, reasoning)` → updates internal status (§5.4).

Status: Open | Acknowledged | Addressed | Dismissed.