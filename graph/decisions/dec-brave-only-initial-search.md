---
id: dec-brave-only-initial-search
type: decision
status: decided
created: 2026-05-04T03:46:15.345788375Z
updated: 2026-05-04T05:01:58.955597106Z
edges:
- target: comp-brave-backend
  type: decision_for
- target: feat-search-router
  type: depended_on_by
---
**Recommended initial search backend: Brave only** (§4.8 *Recommended initial setup*, memory).

Best agentic-search benchmark score (14.89), fastest p50 (669ms), 2k-query free tier, independent index. Add backends in response to a named shortfall, not pre-emptively.

Most likely second-add: **Exa** when code-pattern semantic discovery becomes a frequent query intent.
Most likely third-add: **Firecrawl** when Pickers start needing clean URL extraction (the value is the `extract`/`crawl` shape, not the search shape).

The full §4.8 routing table describes the design surface for forward compatibility, but operational deploys should start narrow.