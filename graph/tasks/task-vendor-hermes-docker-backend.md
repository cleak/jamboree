---
id: task-vendor-hermes-docker-backend
type: task
status: backlog
created: 2026-05-04T03:59:27.576585242Z
updated: 2026-05-04T04:12:56.190762756Z
edges:
- target: feat-sandboxing-profile-x-backend
  type: child_of
---
Phase 4 (§12). Vendor or wrap Hermes Docker backend.

Per `comp-docker-sandbox-backend`, `dec-hermes-as-three-subsystems`.

Acceptance: Picker in `default × docker` runs in a container with read-only repo bind-mount + read-write worktree mount.