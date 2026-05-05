---
id: task-pr-status-poller-etag
type: task
status: backlog
created: 2026-05-04T03:58:49.410657078Z
updated: 2026-05-04T04:11:09.811643701Z
edges:
- target: feat-reviewer-adapters
  type: child_of
---
Phase 2 (§12). ETag-cached PR poller (`jam-pr-poller`).

Per `comp-pr-status-poller`, `metric-pr-poll-etag-304-rate`.

Acceptance: 30s/PR polling; ~70% of polls return 304 in steady state; adaptive cadence drops to 5min for inactive PRs.