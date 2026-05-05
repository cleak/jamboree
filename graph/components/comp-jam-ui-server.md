---
id: comp-jam-ui-server
type: component
status: planned
created: 2026-05-04T03:35:06.772283983Z
updated: 2026-05-04T04:49:06.259027485Z
edges:
- target: comp-ntfy-push-bridge
  type: depends_on
- target: comp-ui-frontend-solidjs
  type: depends_on
- target: comp-ui-session-token-auth
  type: depends_on
- target: comp-ui-websocket-nats-bridge
  type: depends_on
- target: feat-ui-server
  type: used_by
- target: principle-failure-surfaces-immediately
  type: constrained_by
---
Rust + axum, embedded as a process under process-compose (§4.11, §18.1). Crate `crates/jam-ui-server/`.

Endpoints (§4.11):
```
GET  /api/world-snapshot/<task-id>            # cached snapshot
POST /api/world-snapshot/<task-id>/refresh    # force refetch
GET  /api/journal?subject=...&since=...       # paginated journal query
GET  /api/sessions                             # list active sessions
GET  /api/sessions/<id>/transcript            # SSE stream of session output
POST /api/sessions/<id>/messages              # enqueue / interrupt / kill
GET  /api/maestro/state
GET  /api/quotas
GET  /api/trace/<trace-id>
GET  /api/traces/find?filter=...
POST /api/auth/token
WS   /ws                                       # bus event subscription
```

Backend serves the SolidJS SPA as static files. Local-first; optional Tailscale CGNAT exposure for mobile.

Runs as `maestro` user under multi-user model (security-setup §7.6). UI session tokens still attribute actions per-user-id; journal records `from: human:caleb` for UI-initiated actions, not `from: maestro`.