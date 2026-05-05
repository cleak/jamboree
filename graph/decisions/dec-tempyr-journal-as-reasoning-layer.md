---
id: dec-tempyr-journal-as-reasoning-layer
type: decision
status: decided
created: 2026-05-04T03:46:08.249483519Z
updated: 2026-05-04T04:36:54.911468755Z
edges:
- target: feat-tempyr-knowledge-and-journal
  type: depended_on_by
---
**Tempyr journal as the agent reasoning layer** (§v5 changes #4, §22). Tempyr already has an append-only journal with eight typed entry kinds, hybrid retrieval, git-ref publishing, and per-(worktree, agent) sessions. v5 uses it instead of building parallel reasoning storage.

The orchestrator's own JSONL journal narrows to operational events only (§4.4.2). Reasoning lives in Tempyr.

Why: don't duplicate working infrastructure; benefit from `journal_blame`, `journal_range`, and the dead-end-search retrieval pipeline that already exists.

Anchoring strategy (§22.2): Pickers anchor at their own worktree; Maestro anchors at canonical worktree.