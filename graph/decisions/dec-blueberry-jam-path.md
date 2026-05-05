---
id: dec-blueberry-jam-path
type: decision
status: decided
created: 2026-05-04T05:53:50.023548774Z
updated: 2026-05-04T05:55:32.805350590Z
edges:
- target: feat-multi-user-security-model
  type: depended_on_by
- target: feat-tempyr-knowledge-and-journal
  type: depended_on_by
---
**The canonical Tempyr worktree for Blueberry lives at `/home/caleb/blueberry-jam/`** (not `~caleb/code/blueberry-tempyr-live/`).

Path decisions:
- `/home/caleb/blueberry/` — pristine main Blueberry checkout (existing; orchestrator never writes here).
- `/home/caleb/blueberry-jam/` — Jamboree's long-lived `tempyr-live` worktree (mode 2770 caleb:maestro, setgid).
- `/home/picker/workers/<task-id>/` — per-Picker ephemeral worktrees.

Why caleb's home (not maestro's): the worktree is a `git worktree add` of `/home/caleb/blueberry/.git/`, and keeping it adjacent to the main checkout simplifies git operations. Maestro accesses via group permissions (mode 2770 caleb:maestro).

Why `blueberry-jam` (not `blueberry-tempyr-live`): the operational checkout is *for Jamboree*, not for Tempyr per se — Tempyr is just the storage format. The new name is more descriptive.

Note: Blueberry stores its Tempyr graph in-repo at `graph/` (not `tempyr/`) per Blueberry's `CLAUDE.md`. So the orchestrator writes `tempyr/tasks/` lifecycle files at `/home/caleb/blueberry-jam/graph/tasks/` (using Blueberry's existing convention). Caleb-edited content (`graph/nodes/`, `graph/specs/`) stays in the main checkout via normal git operations.