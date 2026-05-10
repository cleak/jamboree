---
id: api-web-extract
type: api_surface
status: stable
created: 2026-05-04T03:52:53.917795092Z
updated: 2026-05-06T23:11:01Z
edges:
- target: comp-jam-svc-search
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`web-extract(urls, render_js?, include_images?)` → extracted page contents (§5.6, §4.8). Implemented by `jam-svc-search` as `tool.search.web-extract` with JSON schemas in `crates/jam-tools-core/schemas/search/`.

The implementation uses a bounded direct HTTP fetcher by default with public
HTTP(S)-only URL validation, local/private host blocking, no redirects, HTML
text/link/image parsing, and traced replies. `render_js=true` routes through
Firecrawl v2 when a Firecrawl credential is configured through env,
`JAM_SECRETS_FILE`, or maestro `pass`; without that key, the request fails fast
with `capability-unavailable`.
