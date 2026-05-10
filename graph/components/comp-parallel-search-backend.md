---
id: comp-parallel-search-backend
type: component
status: active
created: 2026-05-04T03:34:57.544369561Z
updated: 2026-05-06T21:21:00Z
edges:
- target: comp-search-backend-trait
  type: depends_on
- target: feat-deep-research
  type: used_by
- target: feat-search-router
  type: used_by
---
Highest accuracy on HLE-Search and BrowseComp benchmarks; high latency (~13s); reserved for hardest multi-hop research (§4.8). Parallel Pro is fallback for `Deep` research tier (§4.10).

Implementation note (2026-05-06): `jam-svc-research` includes a Parallel
`/v1/tasks/runs` adapter with processor `pro`, including mocked create/poll/result
normalization coverage for the Deep research fallback path.
