---
id: task-ntfy-integration
type: task
status: blocked
created: 2026-05-04T04:00:06.932129532Z
updated: 2026-05-06T19:20:10Z
edges:
- target: feat-ui-server
  type: child_of
---
Phase 6 (§12). ntfy push integration on `notify.human`.

Per `comp-ntfy-push-bridge`.

Implementation note (2026-05-06): the SolidJS UI now opens an authenticated `notify.human` WebSocket stream after connect and surfaces events in a notification drawer with urgency, summary, timestamp, and payload detail. Push delivery is implemented by `jam-ntfy-bridge`; real mobile verification remains tied to ntfy topic/token setup.

Credential note (2026-05-06): local runtime now has `jam/notify/ntfy-token` in
maestro pass, so `jam-ntfy-bridge` can resolve its token without env or
`JAM_SECRETS_FILE`.

Blocked note (2026-05-06): final acceptance still needs mobile topic
subscription verification with the production substrate running. The
`process-compose.yaml` bridge entry remains disabled until that topic is
confirmed; then enable `jam-ntfy-bridge` and verify a `notify.human` event
reaches both push and the UI drawer.
