---
id: comp-jam-svc-observe
type: component
status: planned
created: 2026-05-04T03:31:31.048804392Z
updated: 2026-05-04T04:53:52.450151172Z
edges:
- target: api-branch-staleness
  type: exposes
- target: api-classify-review-artifacts
  type: exposes
- target: api-compute-readiness
  type: exposes
- target: api-list-blockers
  type: exposes
- target: api-list-review-artifacts
  type: exposes
- target: api-query-quota
  type: exposes
- target: api-refresh-world-snapshot
  type: exposes
- target: api-world-snapshot
  type: exposes
- target: api-world-snapshot-delta
  type: exposes
- target: comp-events-toml-and-codegen
  type: depends_on
- target: comp-jam-trace-crate
  type: depends_on
- target: comp-nats-jetstream
  type: depends_on
- target: comp-quota-tracker
  type: depends_on
- target: comp-routing-manifest
  type: depends_on
- target: comp-tempyr-mcp-client-wrapper
  type: depends_on
- target: comp-world-snapshot-cache
  type: depends_on
- target: feat-maestro-tool-surface
  type: used_by
- target: feat-observation-tool-service
  type: used_by
- target: feat-quota-tracking
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
The Rust process `jam-svc-observe` (§4.2). NATS subject prefix `tool.observe.*`. Bin crate at `crates/jam-svc-observe/`.

Provides the typed structures the Maestro uses to reason about reality: `WorldSnapshot`, `ReadinessVerdict`, `Vec<Blocker>`, `Vec<ReviewArtifact>`, `BranchStaleness`, `HarnessQuotaState`.

On startup: validates paths (§2.14), connects NATS, subscribes `tool.observe.*`, loads routing manifest entry, health-pings on `tool.observe.ping` every 5s, refuses to start on any check failure (§2.12).

Subscribes to invalidation events (§21.2) for event-driven cache freshness; backstop 60s TTL.

This is the first tool service to build in Phase 1 (§12) — it's the simplest end-to-end proof of the architecture.