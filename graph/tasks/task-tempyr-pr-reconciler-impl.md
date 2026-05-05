---
id: task-tempyr-pr-reconciler-impl
type: task
status: backlog
created: 2026-05-04T04:01:43.179277924Z
updated: 2026-05-04T04:01:43.179278450Z
---
Implement `tempyr-pr-reconciler` — on `pr.merged`, look at touched paths, query Tempyr for nodes referencing them, emit `tempyr.update-candidate`.

Per `comp-tempyr-pr-reconciler`, `feat-tempyr-consistency-model`.