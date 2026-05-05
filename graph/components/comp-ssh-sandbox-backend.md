---
id: comp-ssh-sandbox-backend
type: component
status: planned
created: 2026-05-04T03:39:25.278088411Z
updated: 2026-05-04T04:44:37.482422747Z
edges:
- target: comp-sandbox-backend-trait
  type: depends_on
- target: feat-sandboxing-profile-x-backend
  type: used_by
- target: principle-native-fs-only
  type: constrained_by
- target: principle-sandbox-blast-radius-not-behavior
  type: constrained_by
---
Remote machine. Hardest isolation; introduces network latency (§6.2). Use case: heavy compute on a beefier machine.

Worktree-only: hard. Remote host has only the worktree; main checkout doesn't exist there (§6.12).

Resource controls via the remote machine's own facilities.