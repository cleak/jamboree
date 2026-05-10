---
id: feat-budget-enforcement
type: feature
status: active
created: 2026-05-04T03:28:16.020500629Z
updated: 2026-05-06T10:17:10.648252635Z
owner: caleb
edges:
- target: comp-ntfy-push-bridge
  type: uses
- target: jamboree-v5
  type: child_of
- target: principle-episodic-maestro-sessions
  type: constrained_by
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
- target: the-manager
  type: serves
---
Three-threshold response (§4.1.4):

| Trigger | Response |
|---|---|
| 100% per-session-usd | Soft-warn: `maestro.budget.soft-exceeded`, finish current turn, abort next unless human extends |
| 125% per-session-usd | Hard-abort: `maestro.budget.hard-exceeded`, dump `~/.jam/maestro-aborted-sessions/<id>.json`, ntfy human |
| 100% daily-usd | Pause-dispatch: set `dispatch-paused: true` in NATS KV, ntfy human urgently |

Resume mechanism: `jam maestro resume <session-id> --budget-extension 5.00` re-wakes with dumped state + fresh budget; `jam maestro abandon <session-id>` discards. No silent continuation.

Implementation note (2026-05-06): hard-abort dump persistence and the CLI resume/abandon surfaces are implemented. The Maestro package exposes a typed `HardAbortDump` writer for `~/.jam/maestro-aborted-sessions/`, and `jam maestro resume` materializes a `~/.jam/maestro-resume-requests/<session-id>.json` request containing the original dump plus the human-approved budget extension.
