---
id: task-ui-shell-axum-and-solidjs
type: task
status: backlog
created: 2026-05-04T03:58:12.779745643Z
updated: 2026-05-04T04:09:27.150938881Z
edges:
- target: feat-ui-server
  type: child_of
---
Phase 0 (§12). UI shell: axum server, SolidJS shell route, WebSocket-to-NATS bridge running. No actual rendering yet.

Per `comp-jam-ui-server`, `comp-ui-frontend-solidjs`.

Acceptance: `jam ui token` issues a session token; visiting `localhost:<port>` with the token over WebSocket connects to NATS and streams a test event.