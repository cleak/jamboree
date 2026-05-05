---
id: feat-tempyr-knowledge-and-journal
type: feature
status: draft
created: 2026-05-04T03:28:17.599768355Z
updated: 2026-05-04T05:55:32.805350117Z
owner: caleb
edges:
- target: api-tempyr-journal-entry-kinds
  type: exposes
- target: comp-canonical-tempyr-worktree
  type: uses
- target: comp-tempyr-mcp-client-wrapper
  type: uses
- target: comp-tempyr-task-node-shape
  type: uses
- target: dec-blueberry-jam-path
  type: depends_on
- target: dec-tempyr-canonical-worktree
  type: depends_on
- target: dec-tempyr-journal-as-reasoning-layer
  type: depends_on
- target: insight-three-checkout-geography
  type: informed_by
- target: jamboree-v5
  type: child_of
- target: principle-adopt-subsystems-not-frameworks
  type: constrained_by
- target: principle-journal-is-sacred-no-compaction
  type: constrained_by
- target: principle-native-fs-only
  type: constrained_by
- target: principle-self-improvement-via-markdown-git-hermes
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
- target: task-tempyr-canonical-worktree-bootstrap
  type: parent_of
- target: task-tempyr-journal-integration-maestro
  type: parent_of
---
Caleb's existing file-based knowledge graph plus journal becomes Jamboree's durable knowledge and agent-reasoning layer (§4.6, §22).

The orchestrator both reads (`query-tempyr`, `tempyr-journal-search`, `tempyr-journal-blame`, `tempyr-journal-range`) and writes (`record-learning`, `record-improvement-candidate`, `record-tempyr-update-candidate`, plus auto-emits on workflow transitions).

Three-checkout geography (§4.6.1):
- `~/code/blueberry/` — pristine main checkout, orchestrator never writes.
- `~/.jam/worktrees/<task-id>/` — Picker worktrees (per-task, ephemeral).
- `~/code/blueberry-tempyr-live/` — canonical Tempyr worktree, orchestrator-owned, long-lived `tempyr-live` branch. Maestro reasoning anchors here; task lifecycle writes here.

Anchoring strategy (§22.2):
- Pickers anchor at their own worktree; agent `picker:<harness>:<handle>`.
- Maestro anchors at canonical worktree; agent `maestro:<session-id>` (per-wake unique).