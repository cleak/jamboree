---
id: task-hard-fs-network-isolation-tests
type: task
status: backlog
created: 2026-05-04T03:59:35.295437965Z
updated: 2026-05-04T04:13:19.925723869Z
edges:
- target: feat-sandboxing-profile-x-backend
  type: child_of
---
Phase 4 (§12) acceptance tests. Hard FS / network isolation.

Acceptance: Picker in `hardened × docker` cannot access files outside its worktree (verified by attempted `ls /` in the Picker turning up only the container's view). Performance regression vs `local × default` is acceptable for the task class (compile-heavy regression < 25%).