---
id: task-hard-abort-dump-and-resume
type: task
status: done
created: 2026-05-04T04:01:24.882738119Z
updated: 2026-05-06T10:17:10.648252635Z
---
Implement Maestro hard-abort dump + resume mechanism (§4.1.4):
- Hard-abort at 125% per-session-usd: dump partial state to `~/.jam/maestro-aborted-sessions/<session-id>.json`.
- `jam maestro resume <session-id> --budget-extension 5.00` re-wakes with dumped state + fresh budget.
- `jam maestro abandon <session-id>` discards.

Per `feat-budget-enforcement`.

Implementation note (2026-05-06): added `jam_maestro.hard_abort.HardAbortDump` plus atomic read/write helpers for the spec dump path `~/.jam/maestro-aborted-sessions/<session-id>.json`. The Rust CLI now implements `jam maestro resume <session-id> --budget-extension <usd>` by validating the abort dump and writing `~/.jam/maestro-resume-requests/<session-id>.json` with the inspected dump plus fresh budget allocation, and implements `jam maestro abandon <session-id>` by deleting the abort dump and any pending resume request. Unit tests cover dump round-trip, session-id traversal rejection, resume request creation, invalid budget rejection, and abandon cleanup.
