---
id: comp-brave-backend
type: component
status: active
created: 2026-05-04T03:34:52.031898047Z
updated: 2026-05-06T10:03:08Z
edges:
- target: comp-search-backend-trait
  type: depends_on
- target: dec-brave-only-initial-search
  type: has_decision
- target: feat-search-router
  type: used_by
---
Latency leader (~669ms p50), independent index. $5–9 per 1K requests. 2k-query free tier. Best agentic-search benchmark score (14.89). Default for fast factual lookups.

Per memory and §4.8 *Recommended initial setup*: **Brave only** for the starter deploy. Add additional backends in response to a named shortfall, not pre-emptively.

Implemented by `jam-svc-search` using the Brave web search endpoint and
`JAM_BRAVE_API_KEY` / `BRAVE_API_KEY`.
