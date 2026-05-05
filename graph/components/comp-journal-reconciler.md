---
id: comp-journal-reconciler
type: component
status: planned
created: 2026-05-04T03:31:40.495764503Z
updated: 2026-05-04T04:46:50.951940739Z
edges:
- target: comp-nats-jetstream
  type: depends_on
- target: comp-orchestrator-jsonl-journal
  type: depends_on
- target: comp-session-store
  type: depends_on
- target: feat-substrate-services
  type: used_by
- target: principle-agent-first-bounded-supervision
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
---
Subscribes to `journal.*`; replays events into the session store with at-least-once delivery (§4.4.6). Crate `crates/jam-journal-reconciler/` (bin).

Idempotent operations; durable consumer offsets. If the session store gets corrupted or schema-migrated, it's rebuilt from the journal.