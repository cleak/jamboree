---
id: api-archive-session
type: api_surface
status: stable
created: 2026-05-04T03:52:08.226536467Z
updated: 2026-05-06T22:38:00Z
edges:
- target: comp-jam-svc-session
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`archive-session(session_id)` → remove a completed session from `jam-svc-session` active state, retain artifacts/worktree, and journal the archive action (§5.2). Running or killing sessions are refused; use `full-stop` first.
