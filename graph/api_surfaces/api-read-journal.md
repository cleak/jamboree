---
id: api-read-journal
type: api_surface
status: draft
created: 2026-05-04T03:53:14.490843123Z
updated: 2026-05-04T04:32:50.601351183Z
edges:
- target: feat-maestro-tool-surface
  type: exposed_by
---
`read-journal(filters)` → query journal directly (§5.8). Rare; usually `query-session-store` is better for full-text.