---
id: comp-ui-frontend-solidjs
type: component
status: active
created: 2026-05-04T03:35:07.793868098Z
updated: 2026-05-06T15:52:03Z
edges:
- target: comp-jam-ui-server
  type: depended_on_by
- target: feat-ui-server
  type: used_by
---
TypeScript + SolidJS + Tailwind, built with Vite. Single-page app, served as static files from the axum server (§18.1).

Routes (§18.2): `/` dashboard, `/tasks/<id>`, `/maestro`, `/journal`, `/traces`, `/quotas`, `/skills`, `/tempyr`, `/health`, `/settings`.

Frontend state management (§18.6): all server state via NATS WebSocket subscriptions (no polling); SolidJS signals for local UI state; world-snapshot cache mirrors backend cache and invalidates on events; optimistic updates for message-mode actions (UI shows `queued` immediately; reverts on backend rejection).

Implementation note (2026-05-06): the first operational route set is active in
`ui/src/main.tsx`: `/`, `/pickers`, `/maestro`, `/journal`, `/traces`,
`/quotas`, `/health`, and `/settings`. These views derive Picker rows, trace
groups, quota rows, Maestro events, notifications, and health state from the
authenticated `/ws` NATS stream. Deferred routes remain separate backlog slices.

Trace detail note (2026-05-06): `/traces/<trace-id>` fetches
`/api/trace/{trace_id}` with the session token and renders the replay chain
back to root plus chronological journal entries.

Message modes note (2026-05-06): the Pickers route has the first unified
composer for Queue / Interrupt / Full-stop. It sends through the authenticated
message endpoint, prompts before arming Full-stop, and shows local/status-event
outbox pills. Queue/interrupt delivery is now live through
`jam-svc-message` and `jam-svc-session` into running Picker stdin; the outbox
keeps statuses monotonic so late bus events cannot move a pill backward from
`delivered` to `queued`.

Lives in `ui/` per §11.1.
