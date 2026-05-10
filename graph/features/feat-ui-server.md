---
id: feat-ui-server
type: feature
status: draft
created: 2026-05-04T03:28:20.030325778Z
updated: 2026-05-06T15:52:03Z
owner: caleb
edges:
- target: comp-jam-ui-server
  type: uses
- target: comp-jam-svc-message
  type: uses
- target: comp-ntfy-push-bridge
  type: uses
- target: comp-ui-frontend-solidjs
  type: uses
- target: comp-ui-session-token-auth
  type: uses
- target: comp-ui-websocket-nats-bridge
  type: uses
- target: jamboree-v5
  type: child_of
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-rust-trusted-core-python-agent-layer
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
- target: task-message-modes-ui
  type: parent_of
- target: task-ntfy-integration
  type: parent_of
- target: task-session-token-auth-impl
  type: parent_of
- target: task-solidjs-frontend-routes
  type: parent_of
- target: task-tailscale-mobile-docs
  type: parent_of
- target: task-trace-replay-view
  type: parent_of
- target: task-ui-shell-axum-and-solidjs
  type: parent_of
- target: task-websocket-nats-subscription
  type: parent_of
- target: the-manager
  type: serves
---
`jam-ui-server` Rust crate (axum) + SolidJS SPA, served as static files (§4.11, §18). Local-first; optional Tailscale CGNAT exposure for mobile.

Real-time: WebSocket → NATS subscription bridge. No polling.

Implementation note (2026-05-06): the first authenticated WebSocket subscription path is active and live-smoked through local NATS. The UI now opens a `notify.human` WebSocket stream after connect and displays the event in a notification drawer. Trace replay is active through `GET /api/trace/{trace_id}` plus `/traces/<trace-id>`. Message-mode UI controls are active through `POST /api/sessions/{session_id}/messages`, `jam-svc-message`, and `jam-svc-session` stdin delivery for running Pickers. Mobile exposure now has a runbook at `docs/runbooks/mobile-tailscale-ui.md` and startup bind-address enforcement for localhost/Tailscale CGNAT.

Auth: session tokens (§4.11.1) + `allow-bind-addrs = ["127.0.0.1", "100.64.0.0/10"]` (localhost + Tailscale CGNAT range). `jam ui token` issues / revokes.

Routes (§18.2): `/` dashboard, `/tasks/<id>`, `/maestro`, `/journal`, `/traces`, `/quotas`, `/skills`, `/tempyr`, `/health`, `/settings`.

Message-modes UX (§18.3): unified composer with Queue / Interrupt / Full-stop modes; Queue is default; Full-stop triggers confirm dialog.

Trace replay UI (§18.4) shows chronological merge of orchestrator and Tempyr journal entries plus state snapshots.

ntfy bridge for push notifications.
