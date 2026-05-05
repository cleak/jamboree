---
id: task-harness-version-pinning
type: task
status: backlog
created: 2026-05-04T03:59:10.627537984Z
updated: 2026-05-04T04:12:09.288417772Z
edges:
- target: feat-picker-layer-three-tier
  type: child_of
---
Phase 3 (§12). Harness version pinning: per-project lockfile, spawn-time check, `harness-version-watcher`, validation tests.

Per `comp-harness-version-lockfile`, `comp-harness-version-watcher`.

Acceptance: bump a harness binary out-of-band; verify `harness-version-watcher` emits the event and Maestro refuses new spawns.