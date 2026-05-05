---
id: api-purge-session
type: api_surface
status: draft
created: 2026-05-04T03:52:10.400726399Z
updated: 2026-05-04T04:54:37.024268194Z
edges:
- target: comp-jam-svc-session
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`purge-session(handle, reason)` → mark abandoned, delete worktree if not preserved, journal the purge reason (§5.2).