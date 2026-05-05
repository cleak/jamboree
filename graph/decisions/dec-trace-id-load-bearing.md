---
id: dec-trace-id-load-bearing
type: decision
status: decided
created: 2026-05-04T03:46:09.653443810Z
updated: 2026-05-04T05:01:22.029016358Z
edges:
- target: comp-jam-trace-crate
  type: decision_for
- target: feat-trace-propagation
  type: depended_on_by
---
**Trace-id propagation as a load-bearing principle** (§v5 changes #5, §2.13, §23). Every NATS message, every tool call, every journal entry carries a `trace_id`. Traces nest via `parent_trace_id`. The principle is "one external trigger, one trace."

Why: chain traceability after the fact is the only way to debug emergent behavior in agent systems; gaps in tracing become unfixable bugs.

Format: ULID, 26-char Base32, time-sortable, pattern `^[0-9A-HJKMNP-TV-Z]{26}$`.