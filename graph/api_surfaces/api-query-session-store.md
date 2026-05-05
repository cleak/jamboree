---
id: api-query-session-store
type: api_surface
status: draft
created: 2026-05-04T03:52:38.580126151Z
updated: 2026-05-04T04:56:23.174157756Z
edges:
- target: comp-jam-svc-knowledge
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`query-session-store(query, time-range?)` → FTS5 results from past sessions (§5.5).

E.g. "find conversations where I dealt with CodeRabbit comments about ECS" returns relevant past sessions.

Hermes-shaped session store schema (§17.2) with `messages_fts` virtual table.