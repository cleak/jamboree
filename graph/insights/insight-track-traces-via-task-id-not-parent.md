---
id: insight-track-traces-via-task-id-not-parent
type: insight
created: 2026-05-04T03:48:12.551799535Z
updated: 2026-05-04T05:05:14.024619403Z
edges:
- target: feat-trace-propagation
  type: informs
---
**Cross-trigger correlation is via `task_id`/`pr_ref`, not `parent_trace_id`** (§24.5).

Easy to get wrong: when the `pr-status-poller` detects a new comment on a PR and emits `pr.review-received`, that opens a NEW root trace. Not a child of the original task spawn trace.

Per `principle-one-trigger-one-trace` — the poller's detection IS an external trigger. Cross-referencing happens via `task_id` and `pr_ref` in payloads.

The "follow the chain" investigation (§23.9) walks back from a merge by:
1. Using merge event's trace to traverse the merge trace.
2. Cross-referencing `picker_handle` and `task_id` mentioned in those events to find earlier traces sharing those identifiers.

Trace chain gives "what happened during this trigger." `task_id` gives "everything that ever happened for this task across all triggers."