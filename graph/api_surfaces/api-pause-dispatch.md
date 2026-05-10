---
id: api-pause-dispatch
type: api_surface
status: stable
created: 2026-05-04T03:53:29.173520441Z
updated: 2026-05-04T04:58:38.302535733Z
edges:
- target: comp-jam-svc-supervise
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`pause-dispatch(reason)` / `resume-dispatch()` (§5.8). Temporarily stops new spawns.

Sets `dispatch-paused: true` in NATS KV bucket `dispatch-state`. Persists across restarts. New Maestro wakes refuse to spawn until `resume-dispatch`.

Triggered automatically on daily-budget-exceeded (§4.1.4) and on patch-agent failure escalation (§20.5 step D).

Implementation note (2026-05-06): the service routes are `tool.supervise.pause-dispatch` with `{reason, changed_by?}` and `tool.supervise.resume-dispatch` with `{changed_by?}`. Both return the persisted dispatch-state record. The CLI continues to write the same KV keys directly for Manager operations.
