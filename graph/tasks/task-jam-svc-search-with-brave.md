---
id: task-jam-svc-search-with-brave
type: task
status: backlog
created: 2026-05-04T03:59:21.919139431Z
updated: 2026-05-04T04:12:40.136761037Z
edges:
- target: feat-search-router
  type: child_of
---
Phase 3.5 (§12). `jam-svc-search` with **Brave only** as initial backend. Auto-routing policy. Cooldown logic.

Per `comp-jam-svc-search`, `comp-search-router`, `comp-brave-backend`, `dec-brave-only-initial-search`.

Acceptance: Maestro calls `web-search`; router picks Brave; result returns; routing envelope present in journal. Force a backend failure: cooldown kicks in, next call fails (no fallback configured); after 1h, primary retried.

Add Exa and Firecrawl only when workload demands them.