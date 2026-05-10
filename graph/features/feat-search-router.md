---
id: feat-search-router
type: feature
status: active
created: 2026-05-04T03:28:18.941829027Z
updated: 2026-05-06T23:11:01Z
owner: caleb
edges:
- target: api-search-backend-contract
  type: exposes
- target: comp-brave-backend
  type: uses
- target: comp-exa-backend
  type: uses
- target: comp-firecrawl-backend
  type: uses
- target: comp-jam-svc-search
  type: uses
- target: comp-linkup-backend
  type: uses
- target: comp-parallel-search-backend
  type: uses
- target: comp-perplexity-sonar-backend
  type: uses
- target: comp-search-backend-trait
  type: uses
- target: comp-search-router
  type: uses
- target: comp-searxng-backend
  type: uses
- target: comp-tavily-backend
  type: uses
- target: dec-brave-only-initial-search
  type: depends_on
- target: jamboree-v5
  type: child_of
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-provider-agnostic-everywhere
  type: constrained_by
- target: principle-untrusted-content-cannot-issue-commands
  type: constrained_by
- target: task-jam-svc-search-with-brave
  type: parent_of
---
Provider-agnostic search with intelligent auto-routing across modern search APIs (§4.8). `jam-svc-search` exposes `web-search`, `web-extract`, `web-crawl`.

Backends configured per-deploy: Brave (latency leader), Firecrawl (search+extract+crawl), Exa (semantic), Linkup (source-backed), Perplexity Sonar (synthesized), Tavily (snippet/RAG), Parallel (highest accuracy multi-hop), SearXNG (self-hosted).

**Recommended initial setup: Brave only** — best agentic-search benchmark (14.89), fastest p50 (669ms), 2k-query free tier, independent index. Add backends in response to a named shortfall, not pre-emptively.

Cooldown: 1h after any backend failure (matches `hermes-web-search-plus`). Failed backend skipped from routing until cooldown expires; if all in chain fail, surface error rather than silently degrading (§2.12).

Routing transparency: every response carries a `routing` envelope explaining backend and reason; logged to journal for skill-evolution training data.

Implementation note (2026-05-06): `jam-svc-search` now implements the active v1
router: Brave default search with cooldown, optional SearXNG and Linkup
`web-search` routes by explicit backend or intent, direct fetch extraction/crawl,
and Firecrawl for JavaScript-capable extraction/crawl.
