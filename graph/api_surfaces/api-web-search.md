---
id: api-web-search
type: api_surface
status: draft
created: 2026-05-04T03:52:51.589838586Z
updated: 2026-05-04T04:57:16.741038513Z
edges:
- target: comp-jam-svc-search
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`web-search(query, intent?, time-range?, domains?)` → `SearchResults` (§5.6, §4.8). Routed via `comp-search-router`.

Default initial deploy: Brave only (per `dec-brave-only-initial-search`). Cooldown 1h after backend failure.

Routing transparency: response carries `routing` envelope with backend choice and reason; logged into journal for skill-evolution training.