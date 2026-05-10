---
id: task-jam-svc-observe-mvp
type: task
status: done
created: 2026-05-04T03:58:15.402076152Z
updated: 2026-05-06T21:14:48Z
edges:
- target: feat-observation-tool-service
  type: child_of
---
Phase 1 (§12). `jam-svc-observe` implementing `world-snapshot`, `compute-readiness`, `list-blockers`, `branch-staleness`. Cache layer with both event-driven invalidation and 60s TTL.

Per `comp-jam-svc-observe`, `comp-world-snapshot`, `comp-world-snapshot-cache`, `comp-compute-readiness`, `comp-branch-staleness`.

Acceptance: Maestro calls `world-snapshot` via NATS, gets a typed response with `freshness` map; second call within 60s returns cached.

Implementation note (2026-05-06): `world-snapshot` was extended past the original MVP to derive Picker session, worktree, and PR facts from local journal JSONL. It also supports a narrow optional GitHub CLI lookup for `task/<task-id>` branches so `world-snapshot.pr` can be populated before the full GitHub App / PR poller stack lands.

Review-surface note (2026-05-06): the observe MVP now covers the review
summary path too. `world-snapshot.review_artifacts` and
`tool.observe.list-review-artifacts` replay `pr.review-received` journal
summaries; `tool.observe.classify-review-artifacts` classifies untrusted review
bodies supplied by the repo comment surface.

Verification note (2026-05-06): focused tests cover review summary replay and
list filtering, and a temporary-NATS smoke verified traced
`tool.observe.list-review-artifacts` request/reply against the built service.

Freshness hardening note (2026-05-06): removed the remaining
`not implemented yet` placeholders from `world-snapshot` freshness. Quota
freshness now reflects the existing journal/config quota reader instead of a
static unavailable marker. Tempyr freshness now derives a
`tempyr_index_cursor` from `journal.tempyr.jsonl`, treats pending writes as
deferred, and surfaces permanently failed writes as a warning blocker. The
`branch-staleness` tool now uses real git probes (`rev-list`, `diff
--name-only`, and `merge-tree --write-tree --name-only`) to report ahead/behind
counts, touched paths, and clean/conflict/unknown mergeability.

Delta note (2026-05-06): `tool.observe.world-snapshot-delta` is now
implemented with generated Pydantic request/response models and a Maestro tool
registry route. The service compares the current snapshot to a cached baseline
when safe, or returns a full snapshot with `full=true` when no reliable
baseline exists.
