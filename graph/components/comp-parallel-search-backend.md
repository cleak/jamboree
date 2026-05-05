---
id: comp-parallel-search-backend
type: component
status: planned
created: 2026-05-04T03:34:57.544369561Z
updated: 2026-05-04T04:44:04.257839798Z
edges:
- target: comp-search-backend-trait
  type: depends_on
- target: feat-deep-research
  type: used_by
- target: feat-search-router
  type: used_by
---
Highest accuracy on HLE-Search and BrowseComp benchmarks; high latency (~13s); reserved for hardest multi-hop research (§4.8). Parallel Pro is fallback for `Deep` research tier (§4.10).