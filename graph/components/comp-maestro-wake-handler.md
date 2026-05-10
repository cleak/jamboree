---
id: comp-maestro-wake-handler
type: component
status: active
created: 2026-05-04T03:31:29.388180038Z
updated: 2026-05-06T21:15:00Z
edges:
- target: comp-nats-jetstream
  type: depends_on
- target: feat-maestro-orchestration-loop
  type: used_by
---
Subscribes to NATS subjects per §4.1.1 and opens a Maestro session per wake. Sources: `pr.review-received`, `picker.errored`, `picker.idle`, `quota.exhausted-soon`, `tempyr.update-candidate`, `skill.under-suspicion`, direct user input via CLI/UI, periodic ticks (default 5min, configurable per project), `stall.escalation`.

Each wake opens a new `trace_id` (§23) inherited from message headers when applicable. Trace travels through every tool call and Tempyr journal entry the session emits.

Implementation note (2026-05-06): `maestro/src/jam_maestro/wake.py` parses
traced `journal.task.requested` wake events, and `python -m jam_maestro
wake-once` / `listen` consume them from NATS before handing them to
`MaestroSessionLoop`. Broader non-task wake subjects remain future hardening.
