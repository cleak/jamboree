---
id: api-find-conflicts
type: api_surface
status: draft
created: 2026-05-04T03:52:13.962045024Z
updated: 2026-05-04T04:54:54.279637349Z
edges:
- target: comp-jam-svc-worktree
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`find-conflicts(worktree-path, target-ref)` → list of conflicting paths (§5.3). Uses `git merge-tree`.