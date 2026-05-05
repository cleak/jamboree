---
id: comp-nats-jetstream
type: component
status: planned
created: 2026-05-04T03:31:35.867957415Z
updated: 2026-05-04T05:02:44.767277902Z
edges:
- target: api-nats-bus-subjects-catalog
  type: exposes
- target: comp-clock-watcher
  type: depended_on_by
- target: comp-harness-version-watcher
  type: depended_on_by
- target: comp-jam-svc-evolve
  type: depended_on_by
- target: comp-jam-svc-knowledge
  type: depended_on_by
- target: comp-jam-svc-message
  type: depended_on_by
- target: comp-jam-svc-observe
  type: depended_on_by
- target: comp-jam-svc-repo
  type: depended_on_by
- target: comp-jam-svc-research
  type: depended_on_by
- target: comp-jam-svc-search
  type: depended_on_by
- target: comp-jam-svc-session
  type: depended_on_by
- target: comp-jam-svc-supervise
  type: depended_on_by
- target: comp-jam-svc-worktree
  type: depended_on_by
- target: comp-journal-reconciler
  type: depended_on_by
- target: comp-maestro-wake-handler
  type: depended_on_by
- target: comp-pr-status-poller
  type: depended_on_by
- target: comp-skill-suspicion-reconciler
  type: depended_on_by
- target: comp-stall-detector
  type: depended_on_by
- target: comp-task-lifecycle-handler
  type: depended_on_by
- target: comp-tempyr-pr-reconciler
  type: depended_on_by
- target: comp-trunk-fetcher
  type: depended_on_by
- target: comp-ui-websocket-nats-bridge
  type: depended_on_by
- target: constraint-single-node-jetstream
  type: constrained_by
- target: dec-single-node-jetstream
  type: has_decision
- target: feat-live-update-flows
  type: used_by
- target: feat-substrate-services
  type: used_by
- target: principle-decoupled-processes-bus
  type: constrained_by
---
JetStream because we need durability for journal events, at-least-once delivery to reconcilers, and a key-value store for the routing manifest (§4.4.1).

Subjects organized by domain (full catalog §21.1): `journal.*`, `picker.<session-id>.*`, `quota.<harness>.<event>`, `tempyr.*`, `evolve.*`, `ui.*`, `notify.human`, `patch.*`, `snapshot.invalidate.<scope>`, `tool.<service>.<method>`, `tool.<service>.ping`.

NATS KV buckets: `routing-manifest` (§20.2), `harness-versions`, `dispatch-state`, `setup-result`, `patch-lock`.

Single-node JetStream (no cluster). TLS not required (loopback-only); auth token-based, generated at install time, stored in `pass`.

Subscription model: durable consumers per service (resume from last-acknowledged offset on restart); ephemeral consumers per Maestro session.