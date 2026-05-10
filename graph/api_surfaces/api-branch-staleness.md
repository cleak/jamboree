---
id: api-branch-staleness
type: api_surface
status: stable
created: 2026-05-04T03:51:59.632914711Z
updated: 2026-05-06T21:18:32Z
edges:
- target: comp-jam-svc-observe
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`branch-staleness(worktree-path)` → `BranchStaleness` (§5.1, §4.2.3, §6.11).

Computed via `git merge-tree`. Returns trunk-sha-at-create, trunk-sha-now, commits-behind/ahead, mergeability, touched-paths.

Maestro decides rebase vs merge vs ignore — never auto-rebase (`principle-no-auto-rebase`).

Implementation note (2026-05-06): `tool.observe.branch-staleness` is implemented in `jam-svc-observe` with configured `JAM_TRUNK_REF` and `JAM_GIT_BIN`, real ahead/behind counts, touched paths, and clean/conflict/unknown mergeability.
