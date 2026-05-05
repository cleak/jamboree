---
id: comp-canonical-tempyr-worktree
type: component
status: planned
created: 2026-05-04T03:34:44.299378743Z
updated: 2026-05-04T05:06:10.294992554Z
edges:
- target: comp-task-lifecycle-handler
  type: depended_on_by
- target: dec-tempyr-canonical-worktree
  type: has_decision
- target: feat-task-tracking-via-lifecycle-transitions
  type: used_by
- target: feat-tempyr-knowledge-and-journal
  type: used_by
- target: insight-three-checkout-geography
  type: relates_to
- target: principle-tracing-chains-end-to-end
  type: constrained_by
---
Long-lived branch `tempyr-live` checked out at `~/code/<project>-tempyr-live/`. Orchestrator-owned (§4.6.1, §6.10).

```
~/code/blueberry-tempyr-live/
├── tempyr/nodes/   ← human-edited (committed)
├── tempyr/specs/   ← human-edited (committed)
└── tempyr/tasks/   ← orchestrator-edited (UNCOMMITTED, journal-derived)
```

Tempyr MCP server reads from here. Maestro's reasoning journal anchors here. Path-scoped ownership: humans write `nodes`/`specs`, orchestrator writes only `tasks`.

Created **once at orchestrator install time** via `git -C ~/code/blueberry worktree add ~/code/blueberry-tempyr-live tempyr-live`. Lives forever. Never goes through the spawn-time worktree-create protocol (§6.9).

If corrupted: `jam tempyr canonical-worktree recreate` removes existing worktree via `git worktree remove --force`, recreates via `git worktree add`, replays journal events with `pr.merged` and `task.*` from orchestrator journal to rebuild `tempyr/tasks/` (§6.10, §13.19). ~10min downtime, no data loss.

Permissions (security-setup §2): mode 2770 with group `maestro`. Setgid bit propagates group ownership to new files.