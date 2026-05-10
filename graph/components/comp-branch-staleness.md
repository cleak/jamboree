---
id: comp-branch-staleness
type: component
status: active
created: 2026-05-04T03:31:33.383540305Z
updated: 2026-05-06T21:08:16Z
edges:
- target: feat-observation-tool-service
  type: used_by
---
`branch-staleness(worktree-path)` (§4.2.3, §6.11) computes via `git merge-tree`. Returns:

```rust
pub struct BranchStaleness {
    pub trunk_sha_at_create: String,
    pub trunk_sha_now: String,
    pub commits_behind: u32,
    pub commits_ahead: u32,
    pub mergeability: Mergeability,  // Clean | Conflicts(Vec<Path>) | Unknown
    pub touched_paths: Vec<PathBuf>,
}
```

Cheap but not free; uses snapshot's TTL caching plus event-driven invalidation. The Maestro sees the staleness and decides whether to rebase, merge, or ignore. **We never auto-rebase** (`principle-no-auto-rebase`).

Implementation note (2026-05-06): active in `jam-svc-observe`.
`tool.observe.branch-staleness` runs `git rev-list --left-right --count`,
`git diff --name-only`, and `git merge-tree --write-tree --name-only` against
the configured trunk ref (`JAM_TRUNK_REF`, default `origin/master`). It returns
ahead/behind counts, touched paths, and `clean` / `conflicts` / `unknown`
mergeability without mutating the worktree.
