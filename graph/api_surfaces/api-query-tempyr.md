---
id: api-query-tempyr
type: api_surface
status: draft
created: 2026-05-04T03:52:36.319524510Z
updated: 2026-05-04T04:56:14.838450132Z
edges:
- target: comp-jam-svc-knowledge
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`query-tempyr(query, scope?)` → typed graph results (§5.5). Wraps Tempyr's MCP server query.

Cached; subscribes to `tempyr.node-changed` events for invalidation (§4.6.4).