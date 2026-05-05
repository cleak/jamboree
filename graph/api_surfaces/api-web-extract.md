---
id: api-web-extract
type: api_surface
status: draft
created: 2026-05-04T03:52:53.917795092Z
updated: 2026-05-04T04:57:26.092174852Z
edges:
- target: comp-jam-svc-search
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`web-extract(urls, render-js?, include-images?)` → `Vec<ExtractedContent>` (§5.6, §4.8). Capability-gated to backends with `extract: true`.

Most likely third-add backend (after Brave + Exa) is Firecrawl, primarily for the `extract`/`crawl` shape.