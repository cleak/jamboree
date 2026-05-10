---
id: task-journal-reconciler-fts5
type: task
status: done
created: 2026-05-04T03:58:35.862774822Z
updated: 2026-05-06T05:27:55Z
edges:
- target: feat-substrate-services
  type: child_of
---
Phase 1 (§12). `journal-reconciler` writing into SQLite/FTS5 session store (Hermes schema).

Per `comp-journal-reconciler`, `comp-session-store`, `comp-hermes-fts5-schema`.

Acceptance: subscribe to `journal.*`; entries appear in `~/.jam/session-store.db` queryable via FTS5; rebuild from journal works after deleting the DB.

Implementation note (2026-05-06): `crates/jam-journal-reconciler` now provides bin `jam-journal-reconciler`. It creates the Hermes-shaped SQLite/FTS5 session store, ingests traced live `journal.>` messages idempotently, rejects live messages without a valid `Trace-Id`, and supports `--rebuild --once` replay from JSONL journal directories. Unit tests cover direct ingestion/idempotency and JSONL replay; command-level smokes verified live NATS subscription -> FTS query and rebuild-after-delete -> FTS query.
