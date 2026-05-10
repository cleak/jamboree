---
id: comp-task-lifecycle-handler
type: component
status: active
created: 2026-05-04T03:31:41.184319371Z
updated: 2026-05-10T00:00:00Z
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
Reconciler-side process that updates Tempyr task nodes on lifecycle transitions (§4.4.6, §4.6.2, §22.4). Subscribes `picker.spawned`, `picker.exited`, `pr.opened`, `pr.merged`, `task.failed`, `task.abandoned`. Emits `tempyr.task-updated`.

Writes to `~/code/<project>-tempyr-live/tempyr/tasks/<task-id>.yaml` only. Path-scoped ownership: humans write `tempyr/nodes/` and `tempyr/specs/`; orchestrator writes only `tempyr/tasks/`.

Lifecycle transitions table → Tempyr fields touched (§4.6.2).

Crate `crates/jam-task-lifecycle/` (bin).

Implementation note (2026-05-06): The crate is implemented as `crates/jam-task-lifecycle` with bin `jam-task-lifecycle`. It currently uses core NATS subscription to traced `journal.>` messages and updates only the configured canonical worktree task directory. It publishes `journal.tempyr.task-updated` after each successful write.

Implementation note (2026-05-10): pre-Picker spawn failures are now durable through `journal.task.failed`. The lifecycle handler marks the task `status: failed` and copies `failure-reason`, `failure-detail`, `failure-at`, and `failure-source` into Tempyr task frontmatter.
