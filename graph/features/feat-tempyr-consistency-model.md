---
id: feat-tempyr-consistency-model
type: feature
status: draft
created: 2026-05-04T03:28:18.262461217Z
updated: 2026-05-04T04:10:36.328014279Z
owner: caleb
edges:
- target: comp-tempyr-mcp-client-wrapper
  type: uses
- target: comp-tempyr-pr-reconciler
  type: uses
- target: jamboree-v5
  type: child_of
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-journal-is-sacred-no-compaction
  type: constrained_by
- target: principle-no-auto-merge
  type: constrained_by
---
Three drift sources, three handling strategies (§4.6.4):

1. **Orchestrator writes that don't reach Tempyr.** Treated as journaled side-effect with retry. `record-learning` writes to JSONL (immediate, durable), Hermes-shaped session store (async, derived), and Tempyr via MCP (async, with retry `[100ms, 500ms, 2s, 10s, 60s]`). After exhaustion, emit `tempyr.write-permanently-failed` and ntfy human.

2. **Tempyr nodes edited directly.** Tempyr's file watcher fires `node-changed`; orchestrator subscribes and invalidates `query-tempyr` cache. `world-snapshot.tempyr_index_cursor` lets `compute-readiness` flag staleness for the Maestro to refresh.

3. **Code changes that invalidate Tempyr claims.** Two layers:
   - Reactive: `record-tempyr-update-candidate` queues a Maestro-flagged proposal.
   - Proactive: `tempyr-pr-reconciler` on `pr.merged` looks at touched paths and queries Tempyr for nodes referencing them; auto-emits `tempyr-update-candidate`.

Source of truth is the journal + Tempyr's own journal; convergence is reconciler-driven; resolution is human (or Maestro-session) review of candidate queue. We never auto-update Tempyr from candidate flags.