---
id: task-message-modes-ui
type: task
status: done
created: 2026-05-04T04:00:00.595385649Z
updated: 2026-05-06T15:52:03Z
edges:
- target: feat-ui-server
  type: child_of
---
Phase 6 (§12). Message modes UI: enqueue, interrupt, full-stop with confirmations.

Per §18.3, `feat-messaging-three-modes`.

Acceptance: send queue/interrupt messages to a running Picker via UI. Switching to **Full-stop** triggers confirm dialog. Status pills appear: `queued` → `delivered` (queue), `interrupt-requested` → `interrupt-accepted` → `delivered` (interrupt), `kill-requested` → `kill-confirmed` (full-stop).

Implementation note (2026-05-06): accepted. `jam-ui-server` accepts
authenticated `POST /api/sessions/{session_id}/messages` for `queue`,
`interrupt`, and `full-stop`, then delegates to
`tool.message.enqueue-message`, `tool.message.interrupt-with-message`, or
`tool.message.full-stop`. `jam-svc-message` publishes traced Picker
command/status subjects and proxies full-stop to `tool.session.full-stop`.
`jam-svc-session` now launches Pickers with piped stdin, subscribes to
`picker.<session-id>.msg.queue|interrupt` before returning `spawn-picker`,
writes framed queue/interrupt messages to the running Picker stdin, and emits
`delivered`, `interrupt-accepted`, or `delivery-failed` status events. The
SolidJS Pickers view has the unified composer, Full-stop confirmation, and
monotonic status pills from POST responses plus
`picker.<session-id>.msg.status` bus events.

Verification (2026-05-06): `scripts/smoke-message-modes-delivery.sh` used
temporary NATS, `jam-svc-session`, `jam-svc-message`, a fake worktree
responder, and a dry-run Picker that reads stdin. It observed queue
`queued -> delivered`, interrupt
`interrupt-requested -> interrupt-accepted -> delivered`, and captured both
message bodies from Picker stdin. Earlier message/UI smoke verified
`full-stop -> kill-confirmed` through the authenticated UI server path.
