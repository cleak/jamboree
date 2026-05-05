---
id: comp-ui-frontend-solidjs
type: component
status: planned
created: 2026-05-04T03:35:07.793868098Z
updated: 2026-05-04T04:48:40.939218025Z
edges:
- target: comp-jam-ui-server
  type: depended_on_by
- target: feat-ui-server
  type: used_by
---
TypeScript + SolidJS + Tailwind, built with Vite. Single-page app, served as static files from the axum server (§18.1).

Routes (§18.2): `/` dashboard, `/tasks/<id>`, `/maestro`, `/journal`, `/traces`, `/quotas`, `/skills`, `/tempyr`, `/health`, `/settings`.

Frontend state management (§18.6): all server state via NATS WebSocket subscriptions (no polling); SolidJS signals for local UI state; world-snapshot cache mirrors backend cache and invalidates on events; optimistic updates for message-mode actions (UI shows `queued` immediately; reverts on backend rejection).

Lives in `ui/` per §11.1.