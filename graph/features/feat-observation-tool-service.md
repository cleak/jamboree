---
id: feat-observation-tool-service
type: feature
status: draft
created: 2026-05-04T03:28:16.305087842Z
updated: 2026-05-04T04:09:34.266171456Z
owner: caleb
edges:
- target: comp-branch-staleness
  type: uses
- target: comp-compute-readiness
  type: uses
- target: comp-jam-svc-observe
  type: uses
- target: comp-review-artifact-classifier
  type: uses
- target: comp-world-snapshot
  type: uses
- target: comp-world-snapshot-cache
  type: uses
- target: jamboree-v5
  type: child_of
- target: principle-decoupled-processes-bus
  type: constrained_by
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-observable-not-deterministic
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
- target: task-jam-svc-observe-mvp
  type: parent_of
---
Rust process `jam-svc-observe` (NATS subject prefix `tool.observe.*`) that compiles current truth into typed structures the Maestro can reason about (§4.2). Provides:

- `world-snapshot` — fact compiler returning `WorldSnapshot` (session, worktree, branch staleness, PR, CI, review artifacts, blockers, readiness, harness quotas, Tempyr cursor, recent dead_ends).
- `compute-readiness`, `list-blockers`, `list-review-artifacts`, `classify-review-artifacts`, `query-quota`, `world-snapshot-delta`, `branch-staleness`.

Cache layer with **event-driven invalidation backed by 60s TTL** (§4.2.1, §21.2). Each data source carries a `freshness` tag.