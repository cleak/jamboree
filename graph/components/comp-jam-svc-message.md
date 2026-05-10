---
id: comp-jam-svc-message
type: component
status: active
created: 2026-05-04T03:39:34.942674631Z
updated: 2026-05-06T15:52:03Z
edges:
- target: api-enqueue-message
  type: exposes
- target: api-full-stop
  type: exposes
- target: api-interrupt-with-message
  type: exposes
- target: comp-events-toml-and-codegen
  type: depends_on
- target: comp-jam-trace-crate
  type: depends_on
- target: comp-jam-ui-server
  type: depended_on_by
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
- target: feat-ui-server
  type: used_by
- target: principle-decoupled-processes-bus
  type: constrained_by
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
---
Message modes tool service. Subject prefix `tool.message.*`. Crate `crates/jam-svc-message/`.

Tools (§5.7): `enqueue-message(session-id, text, from?)`, `interrupt-with-message(session-id, text, from?)`, `full-stop(session-id, reason)`.

NATS subjects (§5.7): `picker.<session-id>.msg.queue|interrupt|kill|status`. Strict ordering per session-id; `kill` takes precedence; `queue`/`interrupt` after kill rejected.

`full-stop` bypasses the harness adapter's normal channel — `jam-svc-supervise` has the process group ID for every Picker; sends signals directly. Adapter-level full-stop is fallback for backends where direct process control is not available (Modal: API call).

Implementation note (2026-05-06): the Rust service boundary is active in
`crates/jam-svc-message/`. It subscribes to `tool.message.*`, requires traced
requests, validates `session_id`/message bodies, publishes traced
`picker.<session-id>.msg.queue|interrupt|kill|status` events, and proxies
`full-stop` through `tool.session.full-stop`. `jam-svc-session` now owns the
Phase 1 queue/interrupt delivery bridge by piping Picker stdin, subscribing to
session-scoped command subjects before `spawn-picker` returns, writing framed
messages to stdin, and publishing `delivered` / `interrupt-accepted` /
`delivery-failed` status updates. Live smoke verified queue and interrupt
delivery to a running dry-run Picker plus prior full-stop confirmation through
`jam-ui-server`.

Remaining refinement: richer per-harness safe-checkpoint cancellation keys
(Codex/Claude/OpenCode-specific interrupt mechanics) still belong in the
harness adapter layer; the Phase 1 bus-to-stdin delivery path is active.
