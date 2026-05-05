---
id: comp-maestro-wake-handler
type: component
status: planned
created: 2026-05-04T03:31:29.388180038Z
updated: 2026-05-04T04:33:39.370440882Z
edges:
- target: comp-nats-jetstream
  type: depends_on
- target: feat-maestro-orchestration-loop
  type: used_by
---
Subscribes to NATS subjects per §4.1.1 and opens a Maestro session per wake. Sources: `pr.review-received`, `picker.errored`, `picker.idle`, `quota.exhausted-soon`, `tempyr.update-candidate`, `skill.under-suspicion`, direct user input via CLI/UI, periodic ticks (default 5min, configurable per project), `stall.escalation`.

Each wake opens a new `trace_id` (§23) inherited from message headers when applicable. Trace travels through every tool call and Tempyr journal entry the session emits.

Implementation entrypoint: `maestro/src/jam_maestro/wake_handler.py::on_wake`. The wake handler builds a `TraceCtx` from headers and runs the session within `maestro_session(session_id, trace_ctx)` async context manager (§24.1).