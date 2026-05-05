---
id: task-journal-reconciler-fts5
type: task
status: backlog
created: 2026-05-04T03:58:35.862774822Z
updated: 2026-05-04T04:10:33.949977055Z
edges:
- target: feat-substrate-services
  type: child_of
---
Phase 1 (§12). `journal-reconciler` writing into SQLite/FTS5 session store (Hermes schema).

Per `comp-journal-reconciler`, `comp-session-store`, `comp-hermes-fts5-schema`.

Acceptance: subscribe to `journal.*`; entries appear in `~/.jam/session-store.db` queryable via FTS5; rebuild from journal works after deleting the DB.