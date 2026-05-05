---
id: comp-jam-svc-search
type: component
status: planned
created: 2026-05-04T03:34:59.456067081Z
updated: 2026-05-04T04:57:35.308594831Z
edges:
- target: api-web-crawl
  type: exposes
- target: api-web-extract
  type: exposes
- target: api-web-search
  type: exposes
- target: comp-events-toml-and-codegen
  type: depends_on
- target: comp-jam-trace-crate
  type: depends_on
- target: comp-nats-jetstream
  type: depends_on
- target: comp-routing-manifest
  type: depends_on
- target: comp-search-router
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

Configuration in `~/.jam/config/search.toml`. Secret keys reference entries in `pass` (e.g., `pass show jam/search/brave`).