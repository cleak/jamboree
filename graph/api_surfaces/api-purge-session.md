---
id: api-purge-session
type: api_surface
status: stable
created: 2026-05-04T03:52:10.400726399Z
updated: 2026-05-06T22:38:00Z
edges:
- target: comp-jam-svc-session
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`purge-session(session_id, reason, preserve_worktree?)` → remove a completed session from `jam-svc-session` state, mark the task abandoned, delete the retained worktree unless preserved, and journal the purge reason (§5.2). Running or killing sessions are refused; use `full-stop` first.
