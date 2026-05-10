---
id: task-jam-svc-search-with-brave
type: task
status: done
created: 2026-05-04T03:59:21.919139431Z
updated: 2026-05-06T23:31:10Z
edges:
- target: feat-search-router
  type: child_of
---
Phase 3.5 (§12). `jam-svc-search` with **Brave only** as initial backend. Auto-routing policy. Cooldown logic.

Per `comp-jam-svc-search`, `comp-search-router`, `comp-brave-backend`, `dec-brave-only-initial-search`.

Acceptance: Maestro calls `web-search`; router picks Brave; result returns; routing envelope present in journal. Force a backend failure: cooldown kicks in, next call fails (no fallback configured); after 1h, primary retried.

Add Exa and Firecrawl only when workload demands them.

Implementation note (2026-05-06): crate `crates/jam-svc-search` now exposes
`tool.search.web-search` with the Brave-only starter backend. It shells through
a configurable curl binary, requires `JAM_BRAVE_API_KEY` / `BRAVE_API_KEY`,
records routing transparency in the response, publishes traced
`search.web-search` journal events, and puts Brave into a 1h cooldown after
backend failure.

Follow-up note (2026-05-06): `web-extract` and `web-crawl` are now active.
They use direct fetch for static public HTTP(S) pages by default and route to
Firecrawl v2 when configured or when `render_js=true` requires JavaScript
rendering.

Additional backend note (2026-05-06): `web-search` can now route to SearXNG
for privacy-sensitive intent and Linkup for source-backed/citation intent when
the corresponding provider env config is present. Brave remains the default
starter backend.

Live smoke (2026-05-06): temporary NATS on port `42401`, `jam-nats-bridge`,
`jam-svc-search`, and a fake Brave curl produced a successful `web-search`
response with `routing.backend=brave` and wrote
`journal/2026-05-06/journal.search.jsonl` with
`event_type=search.web-search`. A forced backend failure returned
`backend-request-failed`; the next request returned `backend-in-cooldown`.

Service smoke script (2026-05-06): `scripts/smoke-search-service.sh` passed.
It expands the earlier manual fake-Brave smoke into repeatable coverage for
Brave success/cooldown, SearXNG privacy routing, Linkup source-backed routing,
Firecrawl extraction, traced replies, and journal landing.
