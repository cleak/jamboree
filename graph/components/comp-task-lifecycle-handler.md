---
id: comp-task-lifecycle-handler
type: component
status: planned
created: 2026-05-04T03:31:41.184319371Z
updated: 2026-05-04T04:47:07.317767947Z
edges:
- target: comp-canonical-tempyr-worktree
  type: depends_on
- target: comp-nats-jetstream
  type: depends_on
- target: comp-tempyr-task-node-shape
  type: depends_on
- target: feat-substrate-services
  type: used_by
- target: feat-task-tracking-via-lifecycle-transitions
  type: used_by
- target: principle-agent-first-bounded-supervision
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
---
Reconciler-side process that updates Tempyr task nodes on lifecycle transitions (§4.4.6, §4.6.2, §22.4). Subscribes `picker.spawned`, `pr.opened`, `pr.merged`, `task.abandoned`. Emits `tempyr.task-updated`.

Writes to `~/code/<project>-tempyr-live/tempyr/tasks/<task-id>.yaml` only. Path-scoped ownership: humans write `tempyr/nodes/` and `tempyr/specs/`; orchestrator writes only `tempyr/tasks/`.

Lifecycle transitions table → Tempyr fields touched (§4.6.2).

Crate `crates/jam-task-lifecycle/` (bin).