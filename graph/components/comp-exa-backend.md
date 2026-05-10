---
id: comp-exa-backend
type: component
status: active
created: 2026-05-04T03:34:53.843710706Z
updated: 2026-05-06T21:21:00Z
edges:
- target: comp-search-backend-trait
  type: depends_on
- target: feat-deep-research
  type: used_by
- target: feat-search-router
  type: used_by
---
Semantic discovery; sub-350ms latency; strong on technical docs and conceptual matching (§4.8). Most likely second-add after Brave when code-pattern semantic discovery becomes a frequent query intent.

Implementation note (2026-05-06): `jam-svc-research` includes a real Exa
`/search` adapter using `type="deep-reasoning"` for the Deep research tier,
with credential loading from env, `JAM_SECRETS_FILE`, or maestro pass.
