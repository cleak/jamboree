---
id: api-full-stop
type: api_surface
status: draft
created: 2026-05-04T03:53:08.106800194Z
updated: 2026-05-04T04:58:19.842440882Z
edges:
- target: comp-jam-svc-message
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
- target: feat-messaging-three-modes
  type: exposed_by
---
`full-stop(session-id, reason)` (§5.7).

Semantics: kill the Picker process now. SIGTERM with 2-second grace, then SIGKILL. Worktree state is whatever it was — we explicitly do not roll back, do not auto-revert, do not auto-commit.

Implementation: bypasses the harness adapter's normal channel. `jam-svc-supervise` has the process group ID for every Picker; sends signals directly. Adapter-level full-stop is fallback for backends where direct process control is not available (Modal: API call).

Side effects:
- Journal entry `picker.killed` with reason and current diff snapshot.
- Tempyr journal session finalized via `tempyr journal finalize` from cleanup path.
- Session marked terminated; subsequent messages rejected with `session-terminated`.
- Maestro receives `picker.killed`; on next wake, sees dead session in `world-snapshot`.
- Worktree preserved with marker file `~/.jam/worktrees/<task-id>/.killed-at-<utc-timestamp>`. Not auto-cleaned.

Confirmation lifecycle: `kill-requested` → `kill-confirmed` (process exited) or `picker-zombie` (grace period elapsed → SIGKILL escalation).

UX intent: "this thing is doing something wrong, stop it now, I'll deal with the wreckage."