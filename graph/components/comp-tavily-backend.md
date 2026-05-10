---
id: comp-tavily-backend
type: component
status: active
created: 2026-05-04T03:34:56.611671636Z
updated: 2026-05-06T21:21:00Z
edges:
- target: comp-search-backend-trait
  type: depends_on
- target: feat-deep-research
  type: used_by
- target: feat-search-router
  type: used_by
---
Snippet-style search, RAG-optimized (§4.8). Tavily `/research` for `Quick` research tier (§4.10).

Implementation note (2026-05-06): `jam-svc-research` includes a real Tavily
`/research` adapter for the Quick research tier, with mocked adapter coverage
and shared runtime credential loading.
