---
id: api-worktree-create-protocol
type: api_surface
status: draft
created: 2026-05-04T03:52:16.155205193Z
updated: 2026-05-04T04:59:50.495627021Z
edges:
- target: comp-worktree-create-protocol
  type: exposed_by
- target: feat-sandboxing-profile-x-backend
  type: exposed_by
---
**Internal** `worktree-create-protocol` runs underneath `spawn-picker` (§5.3, §6.9). Not exposed as a Maestro tool.

Two-mutex protocol (fetch-mutex + worktree-create-mutex) avoids the "stale checkout" failure mode while keeping concurrent spawns fast.