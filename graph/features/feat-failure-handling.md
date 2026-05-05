---
id: feat-failure-handling
type: feature
status: draft
created: 2026-05-04T03:28:22.014346381Z
updated: 2026-05-04T04:37:39.570273428Z
owner: caleb
edges:
- target: comp-clock-watcher
  type: uses
- target: comp-harness-version-watcher
  type: uses
- target: comp-ntfy-push-bridge
  type: uses
- target: comp-patch-agent
  type: uses
- target: comp-skill-suspicion-reconciler
  type: uses
- target: comp-stall-detector
  type: uses
- target: comp-supervisor-process-compose
  type: uses
- target: dec-failure-obvious-load-bearing
  type: depends_on
- target: jamboree-v5
  type: child_of
- target: principle-decoupled-processes-bus
  type: constrained_by
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
- target: task-notify-human-ntfy
  type: parent_of
- target: task-stall-detector-mvp
  type: parent_of
- target: the-manager
  type: serves
---
Component-crash matrix (§10.1) and behavioral-failure matrix (§10.2). System designed so any single component failure does not cascade.

Crashes mostly recover via `process-compose` restart, NATS JetStream durability (resume from last-acknowledged offset), and reconciler replay-from-cursor. Patch agent handles tool-service failures with deterministic-then-LLM recovery.

Behavioral failures detected by reconcilers (stall-detector, quota-tracker, harness-version-watcher, clock-watcher, skill-suspicion-reconciler) emit events; Maestro reacts on next wake.

What the Maestro cannot recover from (§10.3): worktree-root or config-dir corruption, simultaneous all-LLM-down, NATS data loss, malicious skill content, patch agent's pinned deps broken.

Failure-obvious checklist (§10.4) for implementers: every component refuses-to-start on bad env / missing manifest / NATS unreachable; emits `*.failed` events with `error_kind` + `detail` + `trace_id` + remediation hint; on retry exhaustion emits `*.permanently-failed` and ntfy-escalates; never silently degrades.