---
id: task-session-token-auth-impl
type: task
status: backlog
created: 2026-05-04T04:00:03.888363769Z
updated: 2026-05-04T04:14:41.290060239Z
edges:
- target: feat-ui-server
  type: child_of
---
Phase 6 (§12). Session token auth.

Per `comp-ui-session-token-auth`.

Acceptance: `jam ui token` issues / revokes tokens. WebSocket handshake verifies token. Token revocation works.