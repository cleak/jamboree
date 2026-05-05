---
id: task-dispatch-pause-resume
type: task
status: backlog
created: 2026-05-04T04:01:27.211306420Z
updated: 2026-05-04T04:01:27.211306853Z
---
Implement `pause-dispatch(reason)` / `resume-dispatch()` that toggles `dispatch-paused: bool` in NATS KV bucket `dispatch-state`.

Per `api-pause-dispatch`. Triggered automatically on daily-budget-exceeded and on patch-agent failure escalation.