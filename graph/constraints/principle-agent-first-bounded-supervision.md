---
id: principle-agent-first-bounded-supervision
type: constraint
status: active
created: 2026-05-04T03:23:47.922587841Z
updated: 2026-05-04T04:27:36.625010789Z
edges:
- target: comp-clock-watcher
  type: constrains
- target: comp-harness-version-watcher
  type: constrains
- target: comp-journal-reconciler
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
- target: comp-trunk-fetcher
  type: constrains
- target: feat-maestro-orchestration-loop
  type: constrains
- target: feat-substrate-services
  type: constrains
---
**§2.2 Agent-first, with bounded deterministic supervision.**

The Maestro handles novelty. Stall detection, reconciliation, journal-to-session-store indexing, Tempyr drift detection, trace-replay traversal, and skill-suspicion accumulation all run as separate cheap processes that emit events and never make policy decisions. They surface anomalies; the Maestro decides what to do.

*Why:* hardening agent reasoning is brittle; hardening deterministic plumbing is straightforward. Putting determinism where it belongs and judgment where it belongs gives both tools the right strength.