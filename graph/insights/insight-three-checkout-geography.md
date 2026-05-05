---
id: insight-three-checkout-geography
type: insight
created: 2026-05-04T03:48:18.286972142Z
updated: 2026-05-04T05:06:10.294992141Z
edges:
- target: comp-canonical-tempyr-worktree
  type: relates_to
- target: feat-tempyr-knowledge-and-journal
  type: informs
---
**Three-checkout geography solves three constraints simultaneously** (§4.6.1, `dec-tempyr-canonical-worktree`):

| Concern | Without three checkouts | With three checkouts |
|---|---|---|
| Pristine main checkout | Dirtied by orchestrator writes | Clean (orchestrator never writes) |
| Cross-session task visibility | Tempyr MCP can't find Picker worktrees | Canonical worktree is stable read-target |
| Concurrent Maestro/Picker writes | Risk of git conflicts | Path-scoped ownership prevents conflicts |

Path-scoped ownership: humans write `tempyr/nodes/` and `tempyr/specs/` (committed); orchestrator writes only `tempyr/tasks/` (uncommitted, journal-derived).

If canonical worktree is corrupted: ~10min downtime, no data loss because `tempyr/tasks/` is journal-derived.