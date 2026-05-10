---
id: api-web-crawl
type: api_surface
status: stable
created: 2026-05-04T03:52:56.249239778Z
updated: 2026-05-06T23:11:01Z
edges:
- target: comp-jam-svc-search
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`web-crawl(root_url, max_depth, max_pages?, render_js?, include_images?)` → bounded same-origin extracted pages (§5.6, §4.8). Implemented by `jam-svc-search` as `tool.search.web-crawl` with JSON schemas in `crates/jam-tools-core/schemas/search/`.

The implementation uses direct HTTP extraction by default, caps crawl
depth/pages, stays same-origin, and blocks local/private hosts. `render_js=true`
routes through Firecrawl v2 when a Firecrawl credential is configured through
env, `JAM_SECRETS_FILE`, or maestro `pass`; without that key, the request fails
fast with `capability-unavailable`.
