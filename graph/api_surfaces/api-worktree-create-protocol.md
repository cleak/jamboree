---
id: api-worktree-create-protocol
type: api_surface
status: stable
created: 2026-05-04T03:52:16.155205193Z
updated: 2026-05-06T21:29:02Z
edges:
- target: comp-worktree-create-protocol
  type: exposed_by
- target: feat-sandboxing-profile-x-backend
  type: exposed_by
---
**Internal** `worktree-create-protocol` runs underneath `spawn-picker` (§5.3, §6.9). Not exposed as a Maestro tool.

Two-mutex protocol (fetch-mutex + worktree-create-mutex) avoids the "stale checkout" failure mode while keeping concurrent spawns fast.

Implementation note (2026-05-06): `jam-svc-worktree` implements the internal request/reply surface as `tool.worktree.create` and `tool.worktree.worktree-create-protocol`; `jam-svc-session` calls it under `spawn-picker` with traced child context.
