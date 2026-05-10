---
id: api-web-search
type: api_surface
status: stable
created: 2026-05-04T03:52:51.589838586Z
updated: 2026-05-06T23:11:01Z
edges:
- target: comp-jam-svc-search
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`web-search(query, intent?, time-range?, domains?)` → `SearchResults` (§5.6, §4.8). Routed via `comp-search-router`.

Default initial deploy: Brave (per `dec-brave-only-initial-search`). Provider
credentials load from env, `JAM_SECRETS_FILE`, or maestro `pass`. Optional
configured routes use SearXNG for privacy-sensitive intent and Linkup for
source-backed/citation intent. Brave cooldown is 1h after backend failure.

Routing transparency: response carries `routing` envelope with backend choice and reason; logged into journal for skill-evolution training.

The Maestro request contract is generated as `SearchWebSearchRequest` and
routed as `web-search` to `tool.search.web-search`. Successful calls emit
`search.web-search` journal events with backend and routing reason.
