---
id: principle-decoupled-processes-bus
type: constraint
status: active
created: 2026-05-04T03:23:48.198214233Z
updated: 2026-05-04T04:28:05.586102391Z
edges:
- target: comp-jam-svc-evolve
  type: constrains
- target: comp-jam-svc-knowledge
  type: constrains
- target: comp-jam-svc-message
  type: constrains
- target: comp-jam-svc-observe
  type: constrains
- target: comp-jam-svc-repo
  type: constrains
- target: comp-jam-svc-research
  type: constrains
- target: comp-jam-svc-search
  type: constrains
- target: comp-jam-svc-session
  type: constrains
- target: comp-jam-svc-supervise
  type: constrains
- target: comp-jam-svc-worktree
  type: constrains
- target: comp-nats-jetstream
  type: constrains
- target: feat-failure-handling
  type: constrains
- target: feat-hot-patching
  type: constrains
- target: feat-observation-tool-service
  type: constrains
- target: feat-substrate-services
  type: constrains
- target: feat-tool-services-out-of-process
  type: constrains
---
**§2.5 Decoupled processes over a bus.**

Maestro, observation tools, session tools, search router, reviewer adapters, supervisor, reconciler, UI server, skill evolution pipeline, patch agent — all separate processes communicating over NATS JetStream. Crashes are isolated; components restart independently. Workflow is a set of subscribers reacting to events, not a flow diagram.

*Why:* the alternative (one orchestrator binary doing everything) couples failure modes. NATS JetStream gives at-least-once delivery, durable cursors, and crash isolation for free.