---
id: comp-trunk-fetcher
type: component
status: planned
created: 2026-05-04T03:31:42.626556351Z
updated: 2026-05-04T04:46:00.812110906Z
edges:
- target: comp-nats-jetstream
  type: depends_on
- target: feat-live-update-flows
  type: used_by
- target: feat-substrate-services
  type: used_by
- target: principle-agent-first-bounded-supervision
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
---
Periodic `git fetch origin --prune` for each project's trunk every 5min (§4.4.6, §21.3). Emits `branch.trunk-moved`, `branch.staleness-updated`.

Recomputes per-worktree staleness so `world-snapshot.branch_staleness` is fresh without each Maestro session re-fetching.

Crate `crates/jam-trunk-fetcher/` (bin).