---
id: comp-workspace-key-newtype
type: component
status: active
created: 2026-05-04T03:39:28.540862811Z
updated: 2026-05-06T21:26:00Z
edges:
- target: feat-sandboxing-profile-x-backend
  type: used_by
- target: principle-rust-trusted-core-python-agent-layer
  type: constrained_by
---
`WorkspaceKey` newtype with a smart constructor (§6.6 Invariant 3). Any character outside `[A-Za-z0-9._-]` in workspace keys is replaced with `_` before use in paths or shell-equivalent contexts. Checked at the type level: raw strings cannot be used where a `WorkspaceKey` is expected.

Implementation note (2026-05-06): `jam-tools-core::workspace::WorkspaceKey`
now owns the sanitizing smart constructor. `jam-svc-worktree` requires a
`WorkspaceKey` at the worktree path-construction boundary, keeping raw task IDs
out of path joins while preserving the existing strict task-id validation at
the tool contract edge.
