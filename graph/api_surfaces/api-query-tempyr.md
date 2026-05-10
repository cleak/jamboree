---
id: api-query-tempyr
type: api_surface
status: stable
created: 2026-05-04T03:52:36.319524510Z
updated: 2026-05-06T21:46:00Z
edges:
- target: comp-jam-svc-knowledge
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`query-tempyr(query, scope?, max_results?)` → typed graph results (§5.5).

Implemented as the Maestro-local `meta.query-tempyr` tool in `jam_maestro.tempyr_query`. It runs a bounded `tempyr search --json --max-results N` subprocess call without a shell and normalizes the response into typed node hits (`node_id`, `node_type`, `status`, `title`, `score`, `snippet`). This keeps the Maestro on the supported Tempyr CLI surface while leaving MCP-backed caching/invalidation as a future service optimization (§4.6.4).
