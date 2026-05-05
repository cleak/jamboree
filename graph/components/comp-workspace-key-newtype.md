---
id: comp-workspace-key-newtype
type: component
status: planned
created: 2026-05-04T03:39:28.540862811Z
updated: 2026-05-04T04:30:33.513174979Z
edges:
- target: feat-sandboxing-profile-x-backend
  type: used_by
- target: principle-rust-trusted-core-python-agent-layer
  type: constrained_by
---
`WorkspaceKey` newtype with a smart constructor (§6.6 Invariant 3). Any character outside `[A-Za-z0-9._-]` in workspace keys is replaced with `_` before use in paths or shell-equivalent contexts. Checked at the type level: raw strings cannot be used where a `WorkspaceKey` is expected.