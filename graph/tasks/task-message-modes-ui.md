---
id: task-message-modes-ui
type: task
status: backlog
created: 2026-05-04T04:00:00.595385649Z
updated: 2026-05-04T04:14:32.845342932Z
edges:
- target: feat-ui-server
  type: child_of
---
Phase 6 (§12). Message modes UI: enqueue, interrupt, full-stop with confirmations.

Per §18.3, `feat-messaging-three-modes`.

Acceptance: send queue/interrupt messages to a running Picker via UI. Switching to **Full-stop** triggers confirm dialog. Status pills appear: `queued` → `delivered` (queue), `interrupt-requested` → `interrupt-accepted` → `delivered` (interrupt), `kill-requested` → `kill-confirmed` (full-stop).