---
id: api-search-backend-contract
type: api_surface
status: stable
created: 2026-05-04T03:53:36.578051656Z
updated: 2026-05-06T22:20:25Z
edges:
- target: comp-search-backend-trait
  type: exposed_by
- target: feat-search-router
  type: exposed_by
---
Implemented in `crates/jam-tools-core/src/contracts.rs` as `jam_tools_core::contracts::SearchBackend` (§4.8, §19.2). Methods: `id`, `capabilities`, `search`, `extract`, `crawl`, `cost_estimate`, `latency_p50_ms`.

`SearchCapabilities` is a feature set over `SearchCapability`: `Search`, `Extract`, `Crawl`, `Semantic`, `SynthesizedAnswer`, `TimeFiltering`, `DomainFiltering`, `JavascriptRendering`.

Each backend (Brave, Firecrawl, Exa, Linkup, Sonar, Tavily, Parallel, SearXNG) is a separate impl.
