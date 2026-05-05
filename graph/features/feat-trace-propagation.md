---
id: feat-trace-propagation
type: feature
status: draft
created: 2026-05-04T03:28:22.866055410Z
updated: 2026-05-04T05:05:14.024619826Z
owner: caleb
edges:
- target: comp-jam-trace-crate
  type: uses
- target: comp-trace-gap-detector
  type: uses
- target: comp-trace-replay-tool
  type: uses
- target: comp-traced-publish-wrapper
  type: uses
- target: dec-trace-id-load-bearing
  type: depends_on
- target: dec-ulid-for-trace-ids
  type: depends_on
- target: insight-track-traces-via-task-id-not-parent
  type: informed_by
- target: jamboree-v5
  type: child_of
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-one-trigger-one-trace
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
- target: task-trace-replay-tool-prove
  type: parent_of
- target: the-manager
  type: serves
---
**§23: every observable behavior traces backwards to its origin without gaps.**

Trace ID format: ULID. 26-char Base32. Time-sortable. Pattern `^[0-9A-HJKMNP-TV-Z]{26}$`.

Propagation (§23.3): NATS message headers (Trace-Id, Parent-Trace-Id), tool call payloads (top-level), Picker spawn env (`JAM_TRACE_ID`/`JAM_PARENT_TRACE_ID`), Tempyr journal tags (`trace:<id>`, `parent-trace:<id>`), orchestrator journal envelope (top-level fields), skill files (`originated-from-trace`).

`trace-replay(trace_id, max_depth?)` (§23.4) returns chronological merge across orchestrator journal, Tempyr journal entries tagged with the trace, NATS messages indexed by trace, skill files where `originated-from-trace == trace_id`, harness lockfile state, routing manifest history.

Static enforcement (§23.6) via three layers: event-emit helpers require trace_id (no Option), NATS publish wrapper rejects publishes without trace_id, integration tests assert continuity.

**One external trigger, one trace.** Cross-trigger correlation is via `task_id`/`pr_ref`, not parent-trace links (§24.5).