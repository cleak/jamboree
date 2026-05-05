---
id: task-tempyr-write-retry-reconciler
type: task
status: backlog
created: 2026-05-04T04:01:33.572030162Z
updated: 2026-05-04T04:01:33.572030678Z
---
Implement Tempyr write retry: `[100ms, 500ms, 2s, 10s, 60s]` backoff. After exhaustion, emit `tempyr.write-permanently-failed` and ntfy human.

Per `feat-tempyr-consistency-model`.