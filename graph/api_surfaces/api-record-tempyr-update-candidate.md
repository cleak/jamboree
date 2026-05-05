---
id: api-record-tempyr-update-candidate
type: api_surface
status: draft
created: 2026-05-04T03:52:42.338049245Z
updated: 2026-05-04T04:56:41.568994580Z
edges:
- target: comp-jam-svc-knowledge
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`record-tempyr-update-candidate(candidate)` → queues a Tempyr edit proposal (§5.5, §4.6.4 *Reactive*).

Maestro flags "this Tempyr node looks stale relative to what I just read." Tool writes a candidate update into `~/.jam/tempyr-update-queue.jsonl`. Human or periodic Maestro session reviews and accepts/rejects.

We don't auto-update Tempyr from candidate flags.