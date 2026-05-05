---
id: task-skill-suspicion-reconciler-impl
type: task
status: backlog
created: 2026-05-04T03:59:43.930114480Z
updated: 2026-05-04T04:13:43.879304659Z
edges:
- target: feat-self-improvement
  type: child_of
---
Phase 5 (§12). `skill-suspicion-reconciler` watching Tempyr `dead_end` accumulation hourly.

Per `comp-skill-suspicion-reconciler`, `metric-skill-suspicion-threshold`.

Acceptance: hand-craft a skill that's deliberately wrong; run a few Picker tasks that fail in ways logged as Tempyr `dead_end` entries with the skill tagged; verify reconciler emits `skill.under-suspicion` after threshold.