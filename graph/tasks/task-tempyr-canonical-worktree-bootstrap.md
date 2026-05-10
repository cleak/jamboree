---
id: task-tempyr-canonical-worktree-bootstrap
type: task
status: done
created: 2026-05-04T03:58:23.360075938Z
updated: 2026-05-06T19:03:50Z
edges:
- target: feat-tempyr-knowledge-and-journal
  type: child_of
---
Phase 1 (§12). Tempyr canonical worktree bootstrap during `jam setup`. Recovery procedure (`jam tempyr canonical-worktree recreate`) documented and tested.

Per `comp-canonical-tempyr-worktree`, `dec-tempyr-canonical-worktree`.

Acceptance: `jam setup` creates `~/code/<project>-tempyr-live/` via `git worktree add` on `tempyr-live` branch; `jam tempyr canonical-worktree recreate` removes and recreates by replaying journal events with `pr.merged` and `task.*`.

Implementation note (2026-05-06): `jam setup` now ensures the Blueberry canonical worktree after required checks pass. Defaults follow `dec-blueberry-jam-path`: repo `/home/caleb/blueberry`, worktree `/home/caleb/blueberry-jam`, branch `tempyr-live`, graph relpath `graph`; env overrides are `JAM_PROJECT_REPO`, `JAM_BLUEBERRY_REPO`, `JAM_CANONICAL_TEMPYR_WORKTREE` / `JAM_TEMPYR_WORKTREE`, `JAM_TEMPYR_BRANCH`, `JAM_TEMPYR_BASE_REF`, `JAM_GRAPH_RELPATH`, and `JAM_JOURNAL_ROOT`. `jam tempyr canonical-worktree recreate` removes an existing worktree with `git worktree remove --force`, recreates it, clears derived `graph/tasks/` only on replacement, and replays task lifecycle journal envelopes in timestamp/sequence order. Unit test and command-level smoke both verified replay to a merged task node. First-run branch creation is covered: when local `tempyr-live` is missing, the CLI creates it from the explicit base ref or from a detected trunk ref such as `origin/master`, without wiping checked-out task files from the new branch.
