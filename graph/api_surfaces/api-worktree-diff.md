---
id: api-worktree-diff
type: api_surface
status: draft
created: 2026-05-04T03:52:11.765596960Z
updated: 2026-05-04T04:54:46.076564303Z
edges:
- target: comp-jam-svc-worktree
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`worktree-diff(worktree-path, base-ref?)` → unified diff (§5.3). Honors path-prefix invariant — ignores anything outside the worktree.