---
id: task-dispatch-policy-quota-skill-driven
type: task
status: backlog
created: 2026-05-04T03:59:16.231116684Z
updated: 2026-05-04T04:12:25.017933340Z
edges:
- target: feat-quota-tracking
  type: child_of
---
Phase 3 (§12). Dispatch logic: Maestro uses quota and skill files to pick a harness per task.

Per `feat-quota-tracking`, `principle-subscription-friendly-api-when-necessary`, `dec-three-tier-picker-pool`.

Acceptance: spawn 3 Pickers across 3 different harnesses in parallel; each runs in its own worktree, journals to Tempyr correctly, completes or fails cleanly.