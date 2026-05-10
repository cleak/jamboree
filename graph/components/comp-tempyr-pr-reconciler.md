---
id: comp-tempyr-pr-reconciler
type: component
status: active
created: 2026-05-04T03:31:41.918758952Z
updated: 2026-05-06T07:32:48.270890549Z
edges:
- target: comp-nats-jetstream
  type: depends_on
- target: comp-tempyr-mcp-client-wrapper
  type: depends_on
- target: feat-substrate-services
  type: used_by
- target: feat-tempyr-consistency-model
  type: used_by
- target: principle-agent-first-bounded-supervision
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
---
Reconciler-side process. Subscribes `pr.merged`. Looks at touched paths in the merge, queries Tempyr for nodes that reference those paths, emits `tempyr.update-candidate` for each match.

Auto-flag pattern (§4.6.4 *Proactive*). Same queue as Maestro's reactive flag (`record-tempyr-update-candidate`); same human-review path. We never auto-update Tempyr from candidate flags.

Crate `crates/jam-tempyr-pr-reconciler/` (bin).

Implementation note (2026-05-06): active Rust service in `crates/jam-tempyr-pr-reconciler`. Runtime config uses `NATS_URL`, optional `NATS_TOKEN`, `JAM_JOURNAL_ROOT` / `JAM_HOME`, and `JAM_GRAPH_DIR` or the canonical Tempyr worktree pair `JAM_CANONICAL_TEMPYR_WORKTREE` / `JAM_GRAPH_RELPATH`. The service refuses untraced live NATS messages per §2.13 and preserves the incoming PR merge trace on emitted `tempyr.update-candidate` events.
