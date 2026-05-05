---
id: comp-jam-svc-supervise
type: component
status: planned
created: 2026-05-04T03:39:36.110301746Z
updated: 2026-05-04T04:58:38.302535341Z
edges:
- target: api-notify-human
  type: exposes
- target: api-pause-dispatch
  type: exposes
- target: comp-events-toml-and-codegen
  type: depends_on
- target: comp-jam-trace-crate
  type: depends_on
- target: comp-nats-jetstream
  type: depends_on
- target: comp-routing-manifest
  type: depends_on
- target: feat-maestro-tool-surface
  type: used_by
- target: feat-messaging-three-modes
  type: used_by
- target: feat-tool-services-out-of-process
  type: used_by
- target: principle-decoupled-processes-bus
  type: constrained_by
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
---
Supervise/notify tool service. Subject prefix `tool.supervise.*`. Crate `crates/jam-svc-supervise/`.

Owns process group IDs for every Picker (used by `full-stop`). Owns `notify-human` (§5.8) → ntfy bridge. Owns `pause-dispatch(reason)` / `resume-dispatch()` (sets `dispatch-paused` in NATS KV bucket `dispatch-state`).