---
id: feat-substrate-services
type: feature
status: draft
created: 2026-05-04T03:28:16.896623670Z
updated: 2026-05-04T04:39:51.497847399Z
owner: caleb
edges:
- target: api-nats-bus-subjects-catalog
  type: exposes
- target: api-orchestrator-journal-envelope
  type: exposes
- target: comp-clock-watcher
  type: uses
- target: comp-events-toml-and-codegen
  type: uses
- target: comp-harness-version-watcher
  type: uses
- target: comp-journal-reconciler
  type: uses
- target: comp-nats-jetstream
  type: uses
- target: comp-orchestrator-jsonl-journal
  type: uses
- target: comp-patch-agent
  type: uses
- target: comp-pr-status-poller
  type: uses
- target: comp-quota-tracker
  type: uses
- target: comp-session-store
  type: uses
- target: comp-skill-suspicion-reconciler
  type: uses
- target: comp-stall-detector
  type: uses
- target: comp-supervisor-process-compose
  type: uses
- target: comp-task-lifecycle-handler
  type: uses
- target: comp-tempyr-pr-reconciler
  type: uses
- target: comp-time-and-clock
  type: uses
- target: comp-trunk-fetcher
  type: uses
- target: constraint-ntp-sync-required
  type: constrained_by
- target: constraint-single-node-jetstream
  type: constrained_by
- target: dec-no-compaction
  type: depends_on
- target: dec-single-node-jetstream
  type: depends_on
- target: dec-sqlite-fts5-not-postgres
  type: depends_on
- target: jamboree-v5
  type: child_of
- target: principle-agent-first-bounded-supervision
  type: constrained_by
- target: principle-decoupled-processes-bus
  type: constrained_by
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-journal-is-sacred-no-compaction
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
- target: task-cargo-workspace-skeleton
  type: parent_of
- target: task-events-codegen-pipeline
  type: parent_of
- target: task-events-toml-initial-vocabulary
  type: parent_of
- target: task-jam-secrets-pass-and-file-backends
  type: parent_of
- target: task-jam-trace-traceid-and-tracedpublish
  type: parent_of
- target: task-journal-reconciler-fts5
  type: parent_of
- target: task-journal-writer-with-secret-redaction
  type: parent_of
- target: task-nats-jetstream-up
  type: parent_of
---
The Rust services that run continuously and provide the bus, durable storage, quota tracking, supervision, and reconciliation (§4.4):

- NATS JetStream bus (§4.4.1)
- Orchestrator JSONL journal + SQLite/FTS5 session store (§4.4.2)
- Schema versioning via `events.toml` manifest + codegen (§4.4.3)
- Time/clock handling (§4.4.4)
- Quota tracker (§4.4.5)
- Reconcilers and watchers (§4.4.6): stall-detector, journal-reconciler, task-lifecycle-handler, tempyr-pr-reconciler, trunk-fetcher, pr-status-poller, skill-suspicion-reconciler, clock-watcher, harness-version-watcher
- Skill evolution pipeline (§4.4.7)
- Supervisor + patch agent under process-compose (§4.4.8)

These are the cheap deterministic processes that run independently of Maestro judgment per §2.2.