---
id: comp-session-store
type: component
status: planned
created: 2026-05-04T03:31:37.152259599Z
updated: 2026-05-04T05:02:54.277551224Z
edges:
- target: comp-hermes-fts5-schema
  type: depends_on
- target: comp-jam-svc-knowledge
  type: depended_on_by
- target: comp-journal-reconciler
  type: depended_on_by
- target: dec-sqlite-fts5-not-postgres
  type: has_decision
- target: feat-substrate-services
  type: used_by
---
SQLite + FTS5 derived view, optimized for query (§4.4.2, §17.2). Schema lifted from hermes-agent.

Tables: `sessions(id, started_at, ended_at, actor, trace_id, metadata_json)`, `messages(id, session_id, timestamp, role, content, metadata_json)`, `messages_fts` (FTS5 virtual on `messages.content`), `tool_calls(id, message_id, tool_name, arguments_json, result_json, duration_ms)`.

Reconciler subscribes to journal events and replays them into the session store with at-least-once delivery semantics. If the session store gets corrupted or schema-migrated, it's rebuilt from the journal — the journal is sacred (`principle-journal-is-sacred-no-compaction`).

`query-session-store` exposes FTS5 queries to the Maestro. Path `~/.jam/session-store.db`.