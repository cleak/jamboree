---
id: principle-tracing-chains-end-to-end
type: constraint
status: active
created: 2026-05-04T03:23:49.283666902Z
updated: 2026-05-04T04:30:54.423966764Z
edges:
- target: comp-canonical-tempyr-worktree
  type: constrains
- target: comp-clock-watcher
  type: constrains
- target: comp-harness-version-watcher
  type: constrains
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
- target: comp-jam-trace-crate
  type: constrains
- target: comp-journal-reconciler
  type: constrains
- target: comp-litellm-backend
  type: constrains
- target: comp-maestro-process
  type: constrains
- target: comp-maestro-session-loop
  type: constrains
- target: comp-pr-status-poller
  type: constrains
- target: comp-skill-suspicion-reconciler
  type: constrains
- target: comp-stall-detector
  type: constrains
- target: comp-task-lifecycle-handler
  type: constrains
- target: comp-tempyr-pr-reconciler
  type: constrains
- target: comp-trace-gap-detector
  type: constrains
- target: comp-trace-replay-tool
  type: constrains
- target: comp-traced-publish-wrapper
  type: constrains
- target: comp-trunk-fetcher
  type: constrains
- target: feat-budget-enforcement
  type: constrains
- target: feat-failure-handling
  type: constrains
- target: feat-hot-patching
  type: constrains
- target: feat-jam-cli
  type: constrains
- target: feat-maestro-orchestration-loop
  type: constrains
- target: feat-maestro-tool-surface
  type: constrains
- target: feat-messaging-three-modes
  type: constrains
- target: feat-observation-tool-service
  type: constrains
- target: feat-picker-layer-three-tier
  type: constrains
- target: feat-record-learning
  type: constrains
- target: feat-self-improvement
  type: constrains
- target: feat-substrate-services
  type: constrains
- target: feat-task-tracking-via-lifecycle-transitions
  type: constrains
- target: feat-tempyr-knowledge-and-journal
  type: constrains
- target: feat-tool-services-out-of-process
  type: constrains
- target: feat-trace-propagation
  type: constrains
- target: feat-ui-server
  type: constrains
---
**§2.13 Tracing chains, end to end.**

Every observable behavior of the system traces backwards to its origin event without gaps. Trace IDs propagate through every NATS message, every tool call, every journal entry. Failure detection without traceback to root cause is unacceptable.

The principle: **one external trigger, one trace.** A user action, a wake event, a periodic tick, an external webhook — each opens a fresh trace. Subsequent activity within that trigger inherits the trace ID. When a Maestro session spawns a Picker, the Picker's lifetime gets a child trace retaining `parent_trace_id`.

Propagation surfaces: NATS headers, tool call payloads, Picker spawn args (`JAM_TRACE_ID`/`JAM_PARENT_TRACE_ID`), Tempyr journal tags, orchestrator journal envelope, skill `originated_from_trace`. Mechanics in §23.

*Why:* in agent-driven systems, the same observable failure (a bad PR) can have many root causes. Without trace-back, every debugging session starts from scratch. With trace-back, debugging is "follow the chain backwards from outcome to trigger."