---
id: task-solidjs-frontend-routes
type: task
status: done
created: 2026-05-04T03:59:52.632853710Z
updated: 2026-05-06T13:30:42.301136529Z
edges:
- target: feat-ui-server
  type: child_of
---
Phase 6 (§12). SolidJS frontend with all routes (dashboard, Picker detail, Maestro, journal, traces, settings).

Per `comp-ui-frontend-solidjs`, §18.2.

Acceptance: open UI on localhost; see live updates as a Picker progresses.

Implementation note (2026-05-06): `ui/src/main.tsx` now renders route-specific
operational views for dashboard, Pickers, Maestro, journal, traces, quotas,
health, and settings from the shared `/ws` bus stream. Validation served the
fresh `ui/dist` bundle through `jam-ui-server` on localhost with a temporary
token and NATS server; `/pickers` returned the app shell, auth passed, and a
synthetic `journal.picker.spawned` message arrived over the real WebSocket
bridge.
