---
id: api-record-tempyr-update-candidate
type: api_surface
status: stable
created: 2026-05-04T03:52:42.338049245Z
updated: 2026-05-06T21:34:43Z
edges:
- target: comp-jam-svc-knowledge
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`record-tempyr-update-candidate(candidate)` → queues a Tempyr edit proposal (§5.5, §4.6.4 *Reactive*).

Maestro flags "this Tempyr node looks stale relative to what I just read." Tool writes a candidate update into `~/.jam/tempyr-update-queue.jsonl`. Human or periodic Maestro session reviews and accepts/rejects.

We don't auto-update Tempyr from candidate flags.

Implementation note (2026-05-06): `record-tempyr-update-candidate` is a local Maestro meta-tool routed as `meta.record-tempyr-update-candidate`. It appends structured JSONL records to `$JAM_HOME/tempyr-update-candidates.jsonl` for human or later reconciler review.
