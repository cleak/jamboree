---
id: comp-ui-websocket-nats-bridge
type: component
status: planned
created: 2026-05-04T03:35:08.824440807Z
updated: 2026-05-04T04:49:15.055348069Z
edges:
- target: comp-jam-ui-server
  type: depended_on_by
- target: comp-nats-jetstream
  type: depends_on
- target: feat-ui-server
  type: used_by
---
WebSocket → NATS subscription bridge. Real-time, no polling. Frontend subscribes to bus subjects (`picker.<id>.output`, `journal.*`, `notify.human`, etc.); backend forwards messages.

Optimistic updates for message-mode actions: UI shows `queued` immediately on send; reverts on backend rejection (§18.6).