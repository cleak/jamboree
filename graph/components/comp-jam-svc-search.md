---
id: comp-jam-svc-search
type: component
status: active
created: 2026-05-04T03:34:59.456067081Z
updated: 2026-05-06T23:31:10Z
edges:
- target: api-web-crawl
  type: exposes
- target: api-web-extract
  type: exposes
- target: api-web-search
  type: exposes
- target: comp-events-toml-and-codegen
  type: depends_on
- target: comp-firecrawl-backend
  type: depends_on
- target: comp-jam-trace-crate
  type: depends_on
- target: comp-linkup-backend
  type: depends_on
- target: comp-nats-jetstream
  type: depends_on
- target: comp-routing-manifest
  type: depends_on
- target: comp-search-router
  type: depends_on
- target: comp-searxng-backend
  type: depends_on
- target: feat-maestro-tool-surface
  type: used_by
- target: feat-search-router
  type: used_by
- target: feat-tool-services-out-of-process
  type: used_by
- target: principle-decoupled-processes-bus
  type: constrained_by
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
---
The search service process. Subject prefix `tool.search.*`. Crate `crates/jam-svc-search/`.

Tools: `web-search`, `web-extract`, `web-crawl`. Routes via `comp-search-router` per `comp-search-backend-trait`-implementing backends.

Configuration in `~/.jam/config/search.toml` plus environment overrides.
Provider credentials load from env first, then `JAM_SECRETS_FILE`, then
maestro `pass` entries such as `jam/search/brave`.

Implementation lives in `crates/jam-svc-search`: `web-search` defaults to
Brave with traced request/reply, routing journal event, and 1h cooldown. Optional
`web-search` routes cover SearXNG for privacy-sensitive intent and Linkup for
source-backed/citation intent when their env config is present. `web-extract`
and `web-crawl` use direct fetch with public HTTP(S)-only URL validation and
HTML parser-backed text/link/image extraction.

Firecrawl v2 is available as the JavaScript-capable extraction/crawl backend
behind `JAM_SEARCH_EXTRACT_BACKEND=firecrawl` or request-level
`render_js=true`; direct fetch remains the default for static pages.

Smoke coverage (2026-05-06): `scripts/smoke-search-service.sh` starts
temporary NATS, `jam-nats-bridge`, `jam-svc-search`, and local fake Brave,
SearXNG, Linkup, and Firecrawl HTTP backends. It verifies traced request/reply,
Brave success, forced `backend-request-failed`, subsequent
`backend-in-cooldown`, privacy-sensitive SearXNG routing, source-backed Linkup
routing, Firecrawl render-js extraction, and `search.web-search` journal
landing for all successful search backends.
