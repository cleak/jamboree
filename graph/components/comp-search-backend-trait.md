---
id: comp-search-backend-trait
type: component
status: planned
created: 2026-05-04T03:34:50.252455919Z
updated: 2026-05-04T05:00:18.166962268Z
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
Trait every search backend implements (§4.8, §19.2):

```rust
pub trait SearchBackend: Send + Sync {
    fn id(&self) -> BackendId;
    fn capabilities(&self) -> SearchCapabilities;
    fn search(&self, query: SearchQuery) -> Result<SearchResults>;
    fn extract(&self, urls: &[Url]) -> Result<Vec<ExtractedContent>>;
    fn crawl(&self, root: &Url, opts: CrawlOpts) -> Result<CrawlResults>;
    fn cost_estimate(&self, query: &SearchQuery) -> Cost;
    fn latency_p50_ms(&self) -> u32;
}

pub struct SearchCapabilities {
    pub search: bool,
    pub extract: bool,
    pub crawl: bool,
    pub semantic: bool,
    pub synthesized_answer: bool,
    pub time_filtering: bool,
    pub domain_filtering: bool,
    pub javascript_rendering: bool,
}
```