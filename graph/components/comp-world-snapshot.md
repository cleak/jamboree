---
id: comp-world-snapshot
type: component
status: planned
created: 2026-05-04T03:31:31.624662873Z
updated: 2026-05-04T04:05:46.361341408Z
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