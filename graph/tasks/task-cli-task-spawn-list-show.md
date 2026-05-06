---
id: task-cli-task-spawn-list-show
type: task
status: done
created: 2026-05-04T03:58:41.273099282Z
updated: 2026-05-06T02:52:36.502820910Z
edges:
- target: feat-jam-cli
  type: child_of
---
Phase 1 (§12). CLI: `jam task spawn`, `jam task list`, `jam task show`.

Per `comp-jam-cli-binary`, `feat-jam-cli`.

Acceptance: `jam task spawn` opens a root trace, publishes `journal.task.requested`, prints task_id and trace_id; `jam task list` shows live tasks; `jam task show <id>` shows current state.