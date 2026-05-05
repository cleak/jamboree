---
id: dec-sqlite-fts5-not-postgres
type: decision
status: decided
created: 2026-05-04T03:46:24.873007201Z
updated: 2026-05-04T05:02:54.277551678Z
edges:
- target: comp-session-store
  type: decision_for
- target: feat-substrate-services
  type: depended_on_by
---
**SQLite + FTS5 single-file for session store** (§4.4.2, §13.10, §14).

Why: scales fine for one-developer workloads. Single-file backup is trivial. FTS5 is excellent for full-text search. Postgres adds operational complexity (separate process, connection pooling, schema migrations) that doesn't pay off at this scale.

Mitigation against scale concerns: schema and queries written portably; migration to Postgres if needed is straightforward.

Schema lifted from hermes-agent (§17.2) — DDL we apply to our own DB.

If session store gets corrupted: rebuild from journal (the journal is sacred per `principle-journal-is-sacred-no-compaction`).