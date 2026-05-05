---
id: principle-journal-is-sacred-no-compaction
type: constraint
status: active
created: 2026-05-04T03:23:50.431528672Z
updated: 2026-05-04T04:28:44.060594102Z
edges:
- target: comp-events-toml-and-codegen
  type: constrains
- target: comp-orchestrator-jsonl-journal
  type: constrains
- target: feat-event-schema-versioning
  type: constrains
- target: feat-substrate-services
  type: constrains
- target: feat-task-tracking-via-lifecycle-transitions
  type: constrains
- target: feat-tempyr-consistency-model
  type: constrains
- target: feat-tempyr-knowledge-and-journal
  type: constrains
---
**The journal is sacred. No compaction.**

The orchestrator JSONL journal (§4.4.2) is append-only and never compacted. Old event types stay forever; new code emits new types; consumers handle both. Disk is cheap; replay-from-journal is the recovery story.

Schema versioning rules (§4.4.3):
- Additive changes (new optional field) bump `event_subtype_version`. Old consumers ignore unknown fields.
- Breaking changes (remove field, change semantics) introduce a new event type (e.g. `picker.spawned.v2`).

The journal is the source of truth; the SQLite/FTS5 session store is a derived view that can be rebuilt from the journal at any time.