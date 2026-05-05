---
id: comp-jam-svc-worktree
type: component
status: planned
created: 2026-05-04T03:39:31.913035840Z
updated: 2026-05-04T04:54:54.279636957Z
edges:
- target: api-find-conflicts
  type: exposes
- target: api-worktree-diff
  type: exposes
- target: comp-events-toml-and-codegen
  type: depends_on
- target: comp-jam-trace-crate
  type: depends_on
- target: comp-nats-jetstream
  type: depends_on
- target: comp-routing-manifest
  type: depends_on
- target: feat-maestro-tool-surface
  type: used_by
- target: feat-tool-services-out-of-process
  type: used_by
- target: principle-decoupled-processes-bus
  type: constrained_by
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
---
Worktree management tool service. Subject prefix `tool.worktree.*`. Crate `crates/jam-svc-worktree/`.

Tools: `worktree-diff(worktree-path, base-ref?)`, `find-conflicts(worktree-path, target-ref)`. Internal `worktree-create-protocol` runs underneath `spawn-picker` (§5.3, §6.9).

Under multi-user model, worktree-root is `/home/picker/workers/<task-id>/` (mode 700, picker:picker).