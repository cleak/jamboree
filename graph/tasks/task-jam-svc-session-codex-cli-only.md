---
id: task-jam-svc-session-codex-cli-only
type: task
status: backlog
created: 2026-05-04T03:58:18.087050421Z
updated: 2026-05-04T04:09:41.329382897Z
edges:
- target: feat-picker-layer-three-tier
  type: child_of
---
Phase 1 (§12). `jam-svc-session` implementing `spawn-picker` for **Codex CLI only**. `local × default` profile.

Per `comp-jam-svc-session`, `comp-codex-cli-adapter`.

Why Codex CLI first: simplest because of clean Skills/SessionStart hooks (§12 Phase 1, §24.9 step 8).

Acceptance: spawn a Picker, watch it edit code in its worktree, watch it open a PR, see PR show up in `world-snapshot.pr` for that task.