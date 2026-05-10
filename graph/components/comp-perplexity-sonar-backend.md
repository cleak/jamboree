---
id: comp-perplexity-sonar-backend
type: component
status: active
created: 2026-05-04T03:34:55.684469134Z
updated: 2026-05-06T21:21:00Z
edges:
- target: comp-search-backend-trait
  type: depends_on
- target: feat-deep-research
  type: used_by
- target: feat-search-router
  type: used_by
---
Synthesized answer with inline citations (§4.8). Sonar Pro for `Standard` research tier (§4.10); Sonar Reasoning Pro as fallback for `Deep` tier.

Implementation note (2026-05-06): `jam-svc-research` includes Perplexity
Sonar HTTP adapters for `sonar`, `sonar-pro`, and `sonar-reasoning-pro`
research paths, normalized into the shared research output shape.
