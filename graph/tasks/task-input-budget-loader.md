---
id: task-input-budget-loader
type: task
status: backlog
created: 2026-05-04T04:01:21.734483706Z
updated: 2026-05-04T04:01:21.734484232Z
---
Implement session-start input budget loader: assembles input within budget, prioritizing wake-event context > world-snapshot > scoped skills > journal events.

Per `feat-input-budget-management`, `metric-input-tokens-per-session`.

If budget tight: skills truncate first; journal replay second; world-snapshot stays.