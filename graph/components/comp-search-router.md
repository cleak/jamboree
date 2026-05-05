---
id: comp-search-router
type: component
status: planned
created: 2026-05-04T03:34:51.142845148Z
updated: 2026-05-04T04:50:15.444135460Z
edges:
- target: comp-jam-svc-search
  type: depended_on_by
- target: comp-search-backend-trait
  type: depends_on
- target: feat-search-router
  type: used_by
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-provider-agnostic-everywhere
  type: constrained_by
---
`Router` (§4.8) selects backend based on query intent + cooldown state. Default routing policy:

| Query intent | Primary | Fallback chain |
|---|---|---|
| Fast factual lookup | Brave | Firecrawl → Tavily |
| Search + content extract | Firecrawl | Tavily → Linkup |
| Semantic discovery | Exa | — |
| Source-backed answer | Linkup | Perplexity Sonar |
| Synthesized answer w/ citations | Perplexity Sonar | — |
| Multi-hop deep research | Exa Deep Research | Parallel Pro → Sonar Pro |
| Privacy-sensitive | SearXNG | — |

**Cooldown**: 1 hour after any backend failure (matches `hermes-web-search-plus` plugin pattern). Failed backend skipped from routing until cooldown expires; if all in chain fail, surface error rather than silently degrading (§2.12).

**Routing transparency**: every search response carries a `routing` envelope explaining which backend was chosen and why; logged into journal for skill-evolution training data.

Memory: Brave is the recommended primary for §4.8 search-router; Context7 covers §4.9 MCP layer; other providers deferred until workload demands.