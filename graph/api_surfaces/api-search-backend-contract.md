---
id: api-search-backend-contract
type: api_surface
status: draft
created: 2026-05-04T03:53:36.578051656Z
updated: 2026-05-04T05:00:18.166962856Z
edges:
- target: comp-search-backend-trait
  type: exposed_by
- target: feat-search-router
  type: exposed_by
---
The `SearchBackend` trait (§4.8, §19.2). Methods: `id`, `capabilities`, `search`, `extract`, `crawl`, `cost_estimate`, `latency_p50_ms`.

`SearchCapabilities`: `search`, `extract`, `crawl`, `semantic`, `synthesized_answer`, `time_filtering`, `domain_filtering`, `javascript_rendering`.

Each backend (Brave, Firecrawl, Exa, Linkup, Sonar, Tavily, Parallel, SearXNG) is a separate impl.