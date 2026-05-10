---
id: api-worktree-diff
type: api_surface
status: stable
created: 2026-05-04T03:52:11.765596960Z
updated: 2026-05-06T22:02:08Z
edges:
- target: comp-jam-svc-worktree
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`worktree-diff(worktree-path, base-ref?)` → unified diff (§5.3). Honors path-prefix invariant — ignores anything outside the worktree.

Implementation note (2026-05-06): `jam-svc-worktree` implements `tool.worktree.worktree-diff` with the generated `WorktreeWorktreeDiffRequest` contract. It canonicalizes `worktree_path`, requires it to live under the configured worktree root, requires it to be the git top-level, validates `base_ref` as a non-option safe git ref, and returns `changed_files` plus a `git diff --no-ext-diff --no-color <base_ref> --` unified diff. The Maestro registry exposes it as `worktree-diff`.
