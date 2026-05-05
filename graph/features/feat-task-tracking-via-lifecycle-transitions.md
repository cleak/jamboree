---
id: feat-task-tracking-via-lifecycle-transitions
type: feature
status: draft
created: 2026-05-04T03:28:17.930715098Z
updated: 2026-05-04T04:10:26.317055706Z
owner: caleb
edges:
- target: comp-canonical-tempyr-worktree
  type: uses
- target: comp-task-lifecycle-handler
  type: uses
- target: comp-tempyr-task-node-shape
  type: uses
- target: jamboree-v5
  type: child_of
- target: principle-journal-is-sacred-no-compaction
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
- target: task-task-lifecycle-handler
  type: parent_of
---
Tempyr task nodes update on lifecycle transitions, not on every event (§4.6.2). Transitions:

| Transition | Trigger event | Tempyr fields touched |
|---|---|---|
| Spawn | `picker.spawned` | Create node, status=in-progress |
| First output | `picker.first-output` | last-updated |
| PR opened | `pr.opened` | pr-ref, status=in-review |
| Review received | `pr.review-received` | review-summary (counts only), status=addressing-comments if Maestro acts |
| CI status flip | `pr.ci.status-changed` | ci-status, last-updated |
| Merge | `pr.merged` | status=merged, outcome, learnings-recorded refs |
| Abandon | `task.abandoned` | status=abandoned, outcome=reason |

Spawn-time write means tasks appear in Tempyr the moment they exist — not at merge, not at PR open. Coarse durable summary; fine-grained operational data lives in journal/session-store.

Owned by `task-lifecycle-handler` reconciler.