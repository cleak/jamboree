---
id: api-find-conflicts
type: api_surface
status: stable
created: 2026-05-04T03:52:13.962045024Z
updated: 2026-05-06T22:02:08Z
edges:
- target: comp-jam-svc-worktree
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`find-conflicts(worktree-path, target-ref)` → list of conflicting paths (§5.3). Uses `git merge-tree`.

Implementation note (2026-05-06): `jam-svc-worktree` implements `tool.worktree.find-conflicts` with the generated `WorktreeFindConflictsRequest` contract. It applies the same canonical worktree-root and git top-level checks as `worktree-diff`, validates `target_ref`, then runs `git merge-tree --write-tree --name-only --no-messages HEAD <target_ref>` and returns sanitized relative conflicting paths. The Maestro registry exposes it as `find-conflicts`.
