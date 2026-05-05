---
id: comp-tempyr-task-node-shape
type: component
status: planned
created: 2026-05-04T03:34:45.124982348Z
updated: 2026-05-04T04:47:07.317768350Z
edges:
- target: comp-task-lifecycle-handler
  type: depended_on_by
- target: feat-task-tracking-via-lifecycle-transitions
  type: used_by
- target: feat-tempyr-knowledge-and-journal
  type: used_by
---
Tempyr task-node YAML shape (§4.6.2):

```yaml
type: task
id: tasks/2026-05-02-canyon-spline-refactor
title: Refactor canyon generator to use spline-based seam protocols
project: blueberry
status: in-progress | in-review | addressing-comments | merged | abandoned
spawned-at: 2026-05-02T08:15:22Z
last-updated: ...

# operational pointers — for joining with live state
session-id: ...
trace-id: ...
picker-handle: ...
harness: ...
worktree-path: ...

# graph relationships
references: [...]
related-tasks: [...]

# coarse-grained durable state
trunk-sha-at-spawn: ...
pr-ref: ...
ci-status: ...
review-summary: { open-comments: 0, blocking: 0 }
learnings-recorded: [...]

# terminal-only fields
outcome: null
merged-sha: null
```

Coarse on operational details: number of comments not their content; PR ref not the diff; latest CI status not the history. Fine-grained operational data lives in journal/session-store.

Lifecycle-transition writes only — maybe 5–8 per task across full lifetime. Tempyr isn't optimized for high-write-rate operational state.