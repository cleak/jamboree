---
id: api-web-crawl
type: api_surface
status: draft
created: 2026-05-04T03:52:56.249239778Z
updated: 2026-05-04T04:57:35.308595223Z
edges:
- target: comp-jam-svc-search
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`web-crawl(root-url, max-depth, opts)` → for backends that support it (§5.6, §4.8). Capability-gated to backends with `crawl: true`.