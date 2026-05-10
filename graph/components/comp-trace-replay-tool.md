---
id: comp-trace-replay-tool
type: component
status: active
created: 2026-05-04T03:39:56.161791986Z
updated: 2026-05-06T21:54:08Z
edges:
- target: api-find-traces
  type: exposes
- target: api-trace-replay
  type: exposes
- target: feat-trace-propagation
  type: used_by
- target: principle-one-trigger-one-trace
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
---
`trace-replay(trace_id, max_depth?)` (§23.4). Returns a chronological merge of:
- Orchestrator journal entries with this trace_id (sorted by `journal_seq`).
- Tempyr journal entries tagged `trace:<id>` (sorted by ts).
- NATS messages indexed by trace_id (for messages that didn't write to journals — rare but possible).
- Skill files where `originated-from-trace == trace_id` (filesystem search).
- Harness lockfile state at spawn time (resolved via `picker.spawned` event payload).
- Routing manifest at spawn time (resolved via NATS KV history).

Walks parent-trace chain up to `max_depth`.

`find-traces(filter)` searches traces matching pattern (e.g., `harness=codex-cli AND outcome=failed AND since=last-7d`).

Implementation note (2026-05-06): the first durable-journal implementation lives in `crates/jam-ui-server/src/trace_replay.rs` and is reused by both `jam-cli` and `jam-ui-server`. `jam trace replay` reconstructs a parent chain from JSONL journals, while `jam trace find` and `GET /api/traces/find` search grouped trace summaries with the §5.8 filter shape. Tempyr journal, skill-origin, NATS KV snapshot, and rare NATS-only message joins remain future enrichments rather than blockers for the implemented local surface.
