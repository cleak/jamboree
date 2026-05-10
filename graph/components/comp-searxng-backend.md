---
id: comp-searxng-backend
type: component
status: active
created: 2026-05-04T03:34:58.497171302Z
updated: 2026-05-06T23:11:01Z
edges:
- target: comp-jam-svc-search
  type: depended_on_by
- target: comp-search-backend-trait
  type: depends_on
- target: feat-search-router
  type: used_by
---
Self-hosted privacy-respecting metasearch (§4.8). Configured for privacy-sensitive query intents.

Implementation note (2026-05-06): `crates/jam-svc-search` can route
`web-search` to SearXNG when `JAM_SEARCH_WEB_BACKEND=searxng`, or in auto mode
when request intent is privacy-sensitive and `JAM_SEARXNG_ENDPOINT` is
configured. It calls the SearXNG JSON API with `q`, `format=json`, `pageno=1`,
and compatible time-range filters.
