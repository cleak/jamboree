---
id: comp-firecrawl-backend
type: component
status: active
created: 2026-05-04T03:34:52.937533882Z
updated: 2026-05-06T23:11:01Z
edges:
- target: comp-jam-svc-search
  type: depended_on_by
- target: comp-search-backend-trait
  type: depends_on
- target: feat-search-router
  type: used_by
---
Search + extract + crawl in one call (§4.8). Default for general agent-search and full-page-extraction needs.

§4.8 *Recommended initial setup* notes Firecrawl is the most likely third-add (after Exa) when Pickers start needing clean URL extraction — the value is the `extract`/`crawl` shape, not the search shape.

Implementation note (2026-05-06): `crates/jam-svc-search` now supports
Firecrawl v2 for `web-extract` and `web-crawl` when
`JAM_SEARCH_EXTRACT_BACKEND=firecrawl` is set, or when a request asks for
`render_js=true`. The default remains direct fetch; Firecrawl credentials load
from env, `JAM_SECRETS_FILE`, or maestro `pass` key `jam/search/firecrawl`, and
missing credentials return structured capability errors instead of silently
falling back.
