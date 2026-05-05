---
id: task-nats-jetstream-up
type: task
status: backlog
created: 2026-05-04T03:58:02.357054855Z
updated: 2026-05-04T04:08:59.172994705Z
edges:
- target: feat-substrate-services
  type: child_of
---
Phase 0 (§12). NATS JetStream running under `process-compose`. Streams configured. KV buckets (`routing-manifest`, `harness-versions`, `dispatch-state`, `setup-result`, `patch-lock`) created.

Per `comp-nats-jetstream`, `dec-single-node-jetstream`.

Acceptance: smoke test publishes a fake `journal.test` event with a trace_id; verify it lands in the day's JSONL file with the trace_id field.