---
id: comp-world-snapshot
type: component
status: active
created: 2026-05-04T03:31:31.624662873Z
updated: 2026-05-06T21:14:48Z
edges:
- target: feat-observation-tool-service
  type: used_by
---
`world-snapshot(task-id-or-pr-url, max-staleness-secs?)` returns a `WorldSnapshot` (§4.2.1):

```rust
pub struct WorldSnapshot {
    pub task_id: String,
    pub captured_at: DateTime<Utc>,
    pub trace_id: TraceId,
    pub freshness: HashMap<DataSource, FreshnessTag>,
    pub session: Option<SessionState>,
    pub worktree: Option<WorktreeState>,
    pub branch_staleness: Option<BranchStaleness>,
    pub pr: Option<PullRequestState>,
    pub ci: Option<CiState>,
    pub review_artifacts: Vec<ReviewArtifact>,
    pub blockers: Vec<Blocker>,
    pub readiness: ReadinessVerdict,
    pub harness_quotas: HashMap<HarnessId, HarnessQuotaState>,
    pub tempyr_index_cursor: TempyrCursor,
    pub recent_dead_ends: Vec<TempyrJournalRef>,
}
```

The fact compiler. Every Maestro decision starts here per §2.1.

Cached with event-driven invalidation backed by 60s TTL (§21.2).

Implementation note (2026-05-06): `jam-svc-observe` compiles the active
snapshot from the JSONL journal plus local probes. It now includes session,
worktree, branch staleness, PR, CI, quota states, and review summary artifacts
from `pr.review-received` events. Review bodies are intentionally not replayed
from the summary event; callers use `read-pr-comments` for the untrusted body
surface.

Tempyr/quota note (2026-05-06): freshness for quota and Tempyr is now derived
from real local sources. Quota uses `journal.quota.jsonl` plus optional project
quota config. Tempyr uses `journal.tempyr.jsonl` to populate
`tempyr_index_cursor`, mark pending writes deferred, and warn on permanent
write failures.

Delta note (2026-05-06): `world-snapshot-delta` is implemented as a safe
field-level delta over the same snapshot shape. It falls back to full snapshot
fields whenever the cache cannot prove a baseline that is not newer than the
caller-provided `since`.
