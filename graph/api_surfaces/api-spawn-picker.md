---
id: api-spawn-picker
type: api_surface
status: draft
created: 2026-05-04T03:52:01.757278088Z
updated: 2026-05-04T04:54:01.535176270Z
edges:
- target: comp-jam-svc-session
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`spawn-picker(spec: SpawnSpec)` → `PickerHandle` (§5.2, §4.5.1, §24.3).

`SpawnSpec`: task_id, trace_id, parent_trace_id, task_class, worktree_path, sandbox_backend, sandbox_profile, initial_prompt, model_override, reasoning_effort, mcp_servers, skills, budget_usd.

Spawn protocol (§24.3): generate child trace → verify quota → worktree creation → verify harness lockfile → sandbox prep → path safety invariants → bootstrap Tempyr journal → get installation token → get harness secrets via per-harness allowlist → launch (multi-user: via `sudo -n -u picker --preserve-env=...`) → emit `journal.picker.spawned`.