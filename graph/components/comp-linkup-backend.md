---
id: comp-linkup-backend
type: component
status: active
created: 2026-05-04T03:34:54.760301005Z
updated: 2026-05-06T23:11:01Z
edges:
- target: comp-jam-svc-search
  type: depended_on_by
- target: comp-search-backend-trait
  type: depends_on
- target: feat-search-router
  type: used_by
---
Source-backed search with citations (§4.8). Configured per-deploy as needed; deferred from initial setup.

Implementation note (2026-05-06): `crates/jam-svc-search` can route
`web-search` to Linkup when `JAM_SEARCH_WEB_BACKEND=linkup`, or in auto mode
when request intent is source-backed/citation-oriented and
a Linkup credential is configured through env, `JAM_SECRETS_FILE`, or maestro
`pass` key `jam/search/linkup`. It calls Linkup `/v1/search` with
`outputType=searchResults`, optional `includeDomains`, and a bounded
`maxResults` from `JAM_SEARCH_RESULT_COUNT`.
