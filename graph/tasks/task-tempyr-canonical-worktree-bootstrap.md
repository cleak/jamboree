---
id: task-tempyr-canonical-worktree-bootstrap
type: task
status: backlog
created: 2026-05-04T03:58:23.360075938Z
updated: 2026-05-04T04:09:55.902078478Z
edges:
- target: feat-tempyr-knowledge-and-journal
  type: child_of
---
Phase 1 (§12). Tempyr canonical worktree bootstrap during `jam setup`. Recovery procedure (`jam tempyr canonical-worktree recreate`) documented and tested.

Per `comp-canonical-tempyr-worktree`, `dec-tempyr-canonical-worktree`.

Acceptance: `jam setup` creates `~/code/<project>-tempyr-live/` via `git worktree add` on `tempyr-live` branch; `jam tempyr canonical-worktree recreate` removes and recreates by replaying journal events with `pr.merged` and `task.*`.