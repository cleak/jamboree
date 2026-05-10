---
id: comp-search-backend-trait
type: component
status: active
created: 2026-05-04T03:34:50.252455919Z
updated: 2026-05-06T22:20:25Z
edges:
- target: api-search-backend-contract
  type: exposes
- target: comp-brave-backend
  type: depended_on_by
- target: comp-exa-backend
  type: depended_on_by
- target: comp-firecrawl-backend
  type: depended_on_by
- target: comp-linkup-backend
  type: depended_on_by
- target: comp-parallel-search-backend
  type: depended_on_by
- target: comp-perplexity-sonar-backend
  type: depended_on_by
- target: comp-search-router
  type: depended_on_by
- target: comp-searxng-backend
  type: depended_on_by
- target: comp-tavily-backend
  type: depended_on_by
- target: feat-search-router
  type: used_by
---
Shared contract every search backend implements (§4.8, §19.2), now defined in `crates/jam-tools-core/src/contracts.rs`:

```rust
pub trait SearchBackend: Send + Sync {
    fn id(&self) -> BackendId;
    fn capabilities(&self) -> SearchCapabilities;
    fn search(&self, query: SearchQuery) -> ContractResult<SearchResults>;
    fn extract(&self, urls: &[String]) -> ContractResult<Vec<ExtractedContent>>;
    fn crawl(&self, root: &str, opts: CrawlOpts) -> ContractResult<CrawlResults>;
    fn cost_estimate(&self, query: &SearchQuery) -> Cost;
    fn latency_p50_ms(&self) -> u32;
}

pub struct SearchCapabilities {
    pub features: Vec<SearchCapability>,
}
```
