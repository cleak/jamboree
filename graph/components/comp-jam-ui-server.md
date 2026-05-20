---
id: comp-jam-ui-server
type: component
status: active
created: 2026-05-04T03:35:06.772283983Z
updated: 2026-05-06T14:18:59Z
edges:
- target: comp-jam-svc-message
  type: depends_on
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

Implementation note (2026-05-06): active endpoints include `/api/health`,
`/api/auth/check`, `/ws`, authenticated `/api/trace/{trace_id}`, and
authenticated `POST /api/sessions/{session_id}/messages`. The trace endpoint
reads durable JSONL journal files under `$JAM_HOME/journal`, validates ULID
trace IDs, walks parent traces by `parent_trace_id`, and returns chronological
entries with source file/line for the UI replay view. The message endpoint
delegates to `tool.message.enqueue-message`,
`tool.message.interrupt-with-message`, and `tool.message.full-stop` so
message-mode behavior stays behind the service boundary.

Mobile/Tailscale note (2026-05-06): startup rejects `JAM_UI_BIND` addresses
outside `JAM_UI_ALLOW_BIND_ADDRS`, defaulting to `127.0.0.1` and
`100.64.0.0/10` per §4.11.1. Smoke verified `0.0.0.0:8787` fails before
static/NATS setup while `100.64.0.1:8787` passes the bind guard.

Runs as `maestro` user under multi-user model (security-setup §7.6). UI session tokens still attribute actions per-user-id; journal records `from: human:caleb` for UI-initiated actions, not `from: maestro`.

Quota dashboard note (2026-05-18): the active UI route calls authenticated `GET /api/quotas` (with `/api/quota` kept as a compatibility alias), refreshes every 30 seconds, and displays live subscription-harness probe status alongside journal/config-derived remaining quota, API provider/model budgets, and price-event-backed API work such as DeepSeek or OpenRouter.
