---
id: api-query-session-store
type: api_surface
status: stable
created: 2026-05-04T03:52:38.580126151Z
updated: 2026-05-06T21:40:25Z
edges:
- target: comp-jam-svc-knowledge
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`query-session-store(query, time-range?)` → FTS5 results from past sessions (§5.5).

E.g. "find conversations where I dealt with CodeRabbit comments about ECS" returns relevant past sessions.

Hermes-shaped session store schema (§17.2) with `messages_fts` virtual table.

Implementation note (2026-05-06): `query-session-store` is a local Maestro meta-tool routed as `meta.query-session-store`. It performs bounded read-only FTS5 queries against `$JAM_SESSION_STORE_DB` or `$JAM_HOME/session-store.db` and returns session/message hits.
