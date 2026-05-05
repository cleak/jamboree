---
id: comp-firecrawl-backend
type: component
status: planned
created: 2026-05-04T03:34:52.937533882Z
updated: 2026-05-04T04:43:22.681657990Z
edges:
- target: comp-search-backend-trait
  type: depends_on
- target: feat-search-router
  type: used_by
---
Search + extract + crawl in one call (§4.8). Default for general agent-search and full-page-extraction needs.

§4.8 *Recommended initial setup* notes Firecrawl is the most likely third-add (after Exa) when Pickers start needing clean URL extraction — the value is the `extract`/`crawl` shape, not the search shape.