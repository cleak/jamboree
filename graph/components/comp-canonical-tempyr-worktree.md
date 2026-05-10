---
id: comp-canonical-tempyr-worktree
type: component
status: active
created: 2026-05-04T03:34:44.299378743Z
updated: 2026-05-06T19:03:50Z
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
Long-lived branch `tempyr-live` checked out at `/home/caleb/blueberry-jam/`. Orchestrator-owned (§4.6.1, §6.10).

```
/home/caleb/blueberry-jam/
├── graph/nodes/   ← human-edited (committed)
├── graph/specs/   ← human-edited (committed)
└── graph/tasks/   ← orchestrator-edited (UNCOMMITTED, journal-derived)
```

Tempyr MCP server reads from here. Maestro's reasoning journal anchors here. Path-scoped ownership: humans write `nodes`/`specs`, orchestrator writes only `tasks`.

Created **once at orchestrator install time** via `jam setup` or `jam tempyr canonical-worktree recreate`. Lives forever. Never goes through the spawn-time worktree-create protocol (§6.9).

If corrupted: `jam tempyr canonical-worktree recreate` removes existing worktree via `git worktree remove --force`, recreates via `git worktree add`, replays journal events with `pr.merged` and `task.*` from orchestrator journal to rebuild `tempyr/tasks/` (§6.10, §13.19). ~10min downtime, no data loss.

Permissions (security-setup §2): mode 2770 with group `maestro`. Setgid bit propagates group ownership to new files.

Implementation note (2026-05-06): CLI support is implemented in `crates/jam-cli`: `jam setup` ensures the worktree exists, and `jam tempyr canonical-worktree recreate` rebuilds it and replays task lifecycle journal events. The CLI sets mode `2770`; group ownership still depends on the surrounding multi-user bootstrap.

Implementation note (2026-05-06): first-run bootstrap now handles a missing local `tempyr-live` branch. `jam-cli` reuses the local branch when it exists; otherwise it creates it with `git worktree add -b tempyr-live ... <base-ref>`, preferring `JAM_TEMPYR_BASE_REF`, then `origin/tempyr-live`, `origin/master`, `origin/main`, `master`, and `main`. The `recreate` path only clears `graph/tasks/` when replacing an existing worktree; first creation preserves checked-out branch contents.
