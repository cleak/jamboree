---
id: comp-world-snapshot-cache
type: component
status: active
created: 2026-05-04T03:31:32.203640361Z
updated: 2026-05-06T21:14:48Z
edges:
- target: comp-jam-svc-observe
  type: depended_on_by
- target: dec-event-driven-cache-invalidation
  type: has_decision
- target: feat-live-update-flows
  type: used_by
- target: feat-observation-tool-service
  type: used_by
---
Event-driven invalidation backed by 60s TTL (§4.2.1, §21.2). v4 used pure 60s TTL; v5 makes the cache subscribe to events implying staleness:

| Event | Invalidates |
|---|---|
| `pr.review-received{task_id}` | snapshot for that task |
| `pr.ci.status-changed{task_id}` | snapshot for that task |
| `pr.merged{task_id}` | snapshot + all snapshots referencing touched paths |
| `picker.exited{task_id}` / `picker.spawned{task_id}` | snapshot for that task |
| `branch.trunk-moved` | all active task snapshots |
| `tempyr.node-changed` | snapshots that referenced the changed node |
| `harness.version-changed` | quota-state portion of all snapshots |
| `quota.<harness>.<event>` | quota-state portion of all snapshots |

TTL stays as backstop for sources without events. The `freshness` field per data source means the Maestro always knows what's fresh and what's "we haven't heard since."

`refresh-world-snapshot(task-id)` forces refetch.

Implementation note (2026-05-06): active in `jam-svc-observe`. The service
keeps a 60-second in-memory snapshot cache, `refresh-world-snapshot` bypasses
it, and subscriptions to journal/domain events invalidate task-specific or
global cache entries. Unit coverage verifies hit, refresh, and explicit
invalidation behavior.

Delta note (2026-05-06): the same cache is the baseline source for
`world-snapshot-delta`. Delta calls never trust an unsafe baseline; they return
the full snapshot when a precise-enough cached baseline is unavailable.
