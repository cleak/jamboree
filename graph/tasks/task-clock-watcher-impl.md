---
id: task-clock-watcher-impl
type: task
status: backlog
created: 2026-05-04T04:01:39.971263319Z
updated: 2026-05-04T04:01:39.971264Z
---
Implement `clock-watcher` reconciler — verifies NTP sync every 10min; emits `clock.unsynced` on drift.

Per `comp-clock-watcher`, `constraint-ntp-sync-required`.