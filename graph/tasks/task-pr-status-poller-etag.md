---
id: task-pr-status-poller-etag
type: task
status: done
created: 2026-05-04T03:58:49.410657078Z
updated: 2026-05-06T07:02:18Z
edges:
- target: feat-reviewer-adapters
  type: child_of
---
Phase 2 (§12). ETag-cached PR poller (`jam-pr-poller`).

Per `comp-pr-status-poller`, `metric-pr-poll-etag-304-rate`.

Acceptance: 30s/PR polling; ~70% of polls return 304 in steady state; adaptive cadence drops to 5min for inactive PRs.

Implementation note (2026-05-06): `crates/jam-pr-poller` implements the first ETag-cached PR poller daemon. It replays active PRs from `JAM_HOME/journal/**/journal.pr.jsonl`, subscribes to `journal.pr.opened`, polls GitHub `repos/<owner>/<repo>/pulls/<n>` through `gh api -i` with `If-None-Match`, emits `journal.pr.status-changed`, `journal.pr.review-received` on comment count increases, and emits `journal.pr.ci.status-changed` from combined commit status plus check-runs. It uses 30s active cadence by default, drops to 300s after 1800s without activity, and opens fresh root traces for poller-detected state changes.

Live smoke (2026-05-06): temporary NATS root `/tmp/jam-pr-poller-smoke-g6V3Bm` replayed Blueberry PR `cleak/blueberry#383` for task `jamboree-smoke-20260506-0621`. The first poll returned HTTP 200 and emitted `pr.status-changed` trace `01KQY1E6PDN8TC7N2W3J7HCZ7H` plus `pr.ci.status-changed` trace `01KQY1E7MMER48GYQMV9T21EX1` with `ci_status=success`; the next two PR polls returned 304 Not Modified (`not_modified_total=2`, `polls_total=3`, logged `etag_304_rate=0.666`). `jam-svc-observe` then returned `world_snapshot.pr.url=https://github.com/cleak/blueberry/pull/383`, `world_snapshot.ci.status=success`, and `freshness.ci.status=fresh`.
