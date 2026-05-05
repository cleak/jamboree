---
id: task-hard-abort-dump-and-resume
type: task
status: backlog
created: 2026-05-04T04:01:24.882738119Z
updated: 2026-05-04T04:01:24.882738593Z
---
Implement Maestro hard-abort dump + resume mechanism (§4.1.4):
- Hard-abort at 125% per-session-usd: dump partial state to `~/.jam/maestro-aborted-sessions/<session-id>.json`.
- `jam maestro resume <session-id> --budget-extension 5.00` re-wakes with dumped state + fresh budget.
- `jam maestro abandon <session-id>` discards.

Per `feat-budget-enforcement`.