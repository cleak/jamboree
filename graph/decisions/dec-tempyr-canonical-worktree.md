---
id: dec-tempyr-canonical-worktree
type: decision
status: decided
created: 2026-05-04T03:46:05.441122064Z
updated: 2026-05-04T05:01:03.939673896Z
edges:
- target: comp-canonical-tempyr-worktree
  type: decision_for
- target: feat-tempyr-knowledge-and-journal
  type: depended_on_by
---
**Three-checkout geography for Tempyr** (§v5 changes #2, §4.6.1). Task state files live in a dedicated long-lived worktree (`~/code/blueberry-tempyr-live/`) — separate from main checkout (which stays pristine) and from per-task Picker worktrees (which are ephemeral). Orchestrator owns the canonical worktree; humans own the main checkout.

Why: avoids dirtying the pristine reference (Option B), keeps cross-session task visibility (Option A breaks this — Picker worktrees aren't persistent), enables single-writer discipline.

Path-scoped ownership: humans write `tempyr/nodes/` and `tempyr/specs/`; orchestrator writes only `tempyr/tasks/`.