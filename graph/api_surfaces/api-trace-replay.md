---
id: api-trace-replay
type: api_surface
status: stable
created: 2026-05-04T03:53:10.495035023Z
updated: 2026-05-06T21:29:02Z
edges:
- target: comp-trace-replay-tool
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`trace-replay(trace-id, max-depth?)` (§5.8, §23.4). Returns chronological merge of orchestrator and Tempyr journal entries plus referenced state snapshots, sorted by ts.

Walks parent-trace chain up to `max_depth` (default 5). Includes:
- Orchestrator journal entries with this trace_id (sorted by `journal_seq`).
- Tempyr journal entries tagged `trace:<id>` (sorted by ts).
- NATS messages indexed by trace_id (rare).
- Skill files where `originated-from-trace == trace_id`.
- Harness lockfile state at spawn time.
- Routing manifest at spawn time.

Implementation note (2026-05-06): `jam trace replay <trace-id>` is implemented in `jam-cli`, and `jam-ui-server` exposes the authenticated `/api/trace/{trace_id}` view over the same durable JSONL journal chain.
