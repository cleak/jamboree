---
id: task-notify-human-ntfy
type: task
status: backlog
created: 2026-05-04T03:59:19.098639706Z
updated: 2026-05-04T04:12:32.977567750Z
edges:
- target: feat-failure-handling
  type: child_of
---
Phase 3 (§12). `notify-human` via ntfy bridge.

Per `comp-ntfy-push-bridge`, `api-notify-human`.

Acceptance: Maestro calls `notify-human(urgency=high, summary="...")`; ntfy push delivered to phone; UI surfaces same event in notification drawer.