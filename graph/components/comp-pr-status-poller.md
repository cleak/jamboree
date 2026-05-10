---
id: comp-pr-status-poller
type: component
status: active
created: 2026-05-04T03:31:43.346629827Z
updated: 2026-05-06T07:02:18Z
edges:
- target: comp-github-app-client
  type: depends_on
- target: comp-nats-jetstream
  type: depends_on
- target: dec-etag-conditional-requests
  type: has_decision
- target: feat-live-update-flows
  type: used_by
- target: feat-reviewer-adapters
  type: used_by
- target: feat-substrate-services
  type: used_by
- target: principle-agent-first-bounded-supervision
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
---
Polls GitHub `/pulls/<n>` with ETag conditional requests every 30s per active PR (§4.4.6, §4.7.1, §21.3). Adaptive: cadence drops to 5min for PRs with no recent activity (no comments/CI events in past 30min), back up to 30s on activity.

Emits `pr.status-changed`, `pr.review-received`, `pr.ci.status-changed`. ~70% of polls return 304 in steady state with ETag caching.

Each polled response that triggers a state change opens its own root trace (`principle-one-trigger-one-trace`) — review-received's trace is rooted at the poller's detection, not the original task spawn (§24.5).

Crate `crates/jam-pr-poller/` (bin).

Implementation status (2026-05-06): active MVP exists in `crates/jam-pr-poller/`. It uses the installed `gh` CLI for this slice; the shared GitHub App client dependency remains the follow-up path for production auth and higher rate limits.
