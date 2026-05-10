---
id: comp-worktree-create-protocol
type: component
status: active
created: 2026-05-04T03:39:27.441061671Z
updated: 2026-05-06T21:15:00Z
edges:
- target: api-worktree-create-protocol
  type: exposes
- target: dec-worktree-create-protocol-two-mutexes
  type: has_decision
- target: feat-sandboxing-profile-x-backend
  type: used_by
- target: principle-failure-surfaces-immediately
  type: constrained_by
---
Strict protocol to avoid the "stale checkout" failure mode (§6.9). `spawn-picker` runs `worktree-create` underneath:

```text
1. Acquire fetch-mutex (per-repo, NATS-backed lease)
2. Check fetch-cursor:
     if last-fetched(origin) < FETCH-STALENESS-THRESHOLD: skip fetch
     else: git fetch origin --prune --tags; update fetch-cursor
3. Release fetch-mutex
4. Resolve trunk-ref: git rev-parse --verify origin/<trunk-branch>
5. Acquire worktree-create-mutex (per-repo)
6. git worktree add <path> -b task/<task-id> <trunk-sha>
7. Release worktree-create-mutex
8. Journal worktree.created with: trunk-sha, branched-at-utc, fetch-cursor-at-create
```

Two mutexes are separate so 10 concurrent worktree-creates don't all block on a single fetch. Only the one that triggered the fetch holds the fetch-mutex; rest skip step 2.

`FETCH-STALENESS-THRESHOLD` defaults to 60 seconds. When spawning 8 Pickers in 5 seconds, only the first triggers a fetch.

Pickers always branch from `origin/<trunk-branch>`, never from local trunk. If `git pull` was run on main checkout an hour ago and broke something, doesn't propagate to new worktrees.

If `git fetch` fails — fail spawn with `worktree-create-failed`; let Maestro decide. Don't fall back to local trunk silently.

Implemented in `jam-svc-worktree`. Crate `crates/jam-svc-worktree/`.
