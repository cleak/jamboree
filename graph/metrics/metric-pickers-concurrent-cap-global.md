---
id: metric-pickers-concurrent-cap-global
type: metric
status: proposed
created: 2026-05-04T03:47:55.586747465Z
updated: 2026-05-04T03:47:55.586748300Z
---
**Global concurrency cap**: 8 Pickers concurrent (§6.7) for Caleb's machine. Tunable.

Per-task-class caps for Blueberry:
- planning, review, summarization: 20
- light-edit, doc-generation, shader-variant: 8
- compile-heavy-rust, gameplay-change, ecs-refactor: 3
- risky-architecture: 1

Substrate enforces the cap mechanically (won't let `spawn-picker` exceed it).