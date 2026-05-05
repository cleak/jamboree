---
id: dec-etag-conditional-requests
type: decision
status: decided
created: 2026-05-04T03:46:21.234114622Z
updated: 2026-05-04T05:02:36.159467299Z
edges:
- target: comp-pr-status-poller
  type: decision_for
- target: feat-reviewer-adapters
  type: depended_on_by
---
**ETag-based conditional requests for GitHub API polling** (§4.7.1). Each PR poll caches the response ETag; subsequent polls send `If-None-Match` and get 304 (no rate limit consumed) when nothing changed.

Steady state: ~70% of polls return 304.

Defense-in-depth on top of GitHub App's elevated rate limits.