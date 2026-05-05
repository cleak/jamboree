---
id: dec-worktree-create-protocol-two-mutexes
type: decision
status: decided
created: 2026-05-04T03:46:35.517234003Z
updated: 2026-05-04T05:03:31.621094165Z
edges:
- target: comp-worktree-create-protocol
  type: decision_for
---
**Worktree creation protocol with two separate mutexes** (§6.9):
1. `fetch-mutex` — per-repo, NATS-backed lease.
2. `worktree-create-mutex` — per-repo.

Why two: 10 concurrent worktree-creates don't all block on a single fetch. Only the one that triggered the fetch holds the fetch-mutex; the rest skip step 2 (fetch).

`FETCH-STALENESS-THRESHOLD` defaults to 60 seconds. When spawning 8 Pickers in 5 seconds, only the first triggers a fetch.

Pickers always branch from `origin/<trunk-branch>`, never from local trunk.

If `git fetch` fails: fail spawn with `worktree-create-failed`. Don't fall back to local trunk silently (§2.12).