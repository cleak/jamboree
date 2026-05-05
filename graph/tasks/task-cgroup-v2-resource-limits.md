---
id: task-cgroup-v2-resource-limits
type: task
status: backlog
created: 2026-05-04T03:59:32.441806868Z
updated: 2026-05-04T04:13:11.639295742Z
edges:
- target: feat-sandboxing-profile-x-backend
  type: child_of
---
Phase 4 (§12). cgroup v2 resource limits for local-backend Pickers.

Per `comp-local-sandbox-backend`, §6.4.

Acceptance: CPU/memory caps enforced per task class; risky-architecture profile ionice class 3 (idle).