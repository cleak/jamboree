---
id: task-jam-svc-worktree-creation-protocol
type: task
status: backlog
created: 2026-05-04T03:58:20.723985368Z
updated: 2026-05-04T04:09:48.319983471Z
edges:
- target: feat-sandboxing-profile-x-backend
  type: child_of
---
Phase 1 (§12). `jam-svc-worktree` implementing the worktree creation protocol (§6.9) with both mutexes (fetch + create).

Per `comp-jam-svc-worktree`, `comp-worktree-create-protocol`, `dec-worktree-create-protocol-two-mutexes`.

Acceptance: spawn 8 Pickers in 5 seconds; verify only the first triggers `git fetch`; all 8 worktrees created from `origin/<trunk>`.