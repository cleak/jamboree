---
id: comp-jam-svc-observe
type: component
status: active
created: 2026-05-04T03:31:31.048804392Z
updated: 2026-05-06T21:14:48Z
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

Implementation note (2026-05-06): `query-quota` now returns journal-derived quota states augmented by optional project-config quota metadata, and `world-snapshot` includes the same map under `harness_quotas`, with quota freshness marked fresh when journal or config states are present. The quota reader also folds `quota.usage-observed` cost into configured API-budget spend.

Review note (2026-05-06): `world-snapshot` now replays
`pr.review-received` summary events from `journal.pr.jsonl` into
`review_artifacts` and marks `freshness["review-artifacts"]` fresh when review
summaries are present. Full comment bodies still come from `read-pr-comments`
and remain untrusted.

List-review note (2026-05-06): `tool.observe.list-review-artifacts` now uses
the same journal summary source, filters by `pr-ref` and `status-filter`, and
returns summary records with `body_trust: untrusted` instead of the previous
outside-MVP error.

Freshness hardening note (2026-05-06): `world-snapshot` no longer reports
quota or Tempyr as static unimplemented sources. It folds the existing quota
journal/config reader into `freshness["quota"]`, derives
`tempyr_index_cursor` from `journal.tempyr.jsonl`, and warns when Tempyr write
retry has permanently failed. `branch-staleness` now runs local git probes and
returns clean/conflict/unknown mergeability instead of a placeholder.

Delta note (2026-05-06): `world-snapshot-delta` now returns a conservative
field-level delta from the cached baseline, falling back to `full=true` when
the baseline is absent or newer than the caller's `since`.
