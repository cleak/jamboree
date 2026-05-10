---
id: comp-hermes-fts5-schema
type: component
status: active
created: 2026-05-04T03:39:52.309890601Z
updated: 2026-05-06T21:22:00Z
edges:
- target: comp-session-store
  type: depended_on_by
- target: dec-hermes-as-three-subsystems
  type: has_decision
- target: principle-adopt-subsystems-not-frameworks
  type: constrained_by
---
Hermes Agent's SQLite + FTS5 schema for conversational session storage (§17.2). DDL only — we apply it to our own DB. No code dependency on Hermes.

```sql
CREATE TABLE sessions (
    id TEXT PRIMARY KEY, started_at TEXT NOT NULL, ended_at TEXT,
    actor TEXT NOT NULL, trace_id TEXT NOT NULL, metadata_json TEXT
);
CREATE TABLE messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    timestamp TEXT NOT NULL, role TEXT NOT NULL, content TEXT NOT NULL,
    metadata_json TEXT
);
CREATE VIRTUAL TABLE messages_fts USING fts5(content, content='messages', content_rowid='id');
CREATE TABLE tool_calls (
    id INTEGER PRIMARY KEY AUTOINCREMENT, message_id INTEGER NOT NULL REFERENCES messages(id),
    tool_name TEXT NOT NULL, arguments_json TEXT NOT NULL, result_json TEXT, duration_ms INTEGER
);
```

Reconciler subscribes to `journal.*` events and replays into this schema. `query-session-store` is FTS5 on the `messages_fts` virtual table.

Implementation note (2026-05-06): `jam-journal-reconciler` creates the
Hermes-derived SQLite/FTS5 tables and populates sessions, messages, and
tool_calls from journal envelopes. The Python `query_session_store` wrapper
queries `messages_fts` with typed results.
