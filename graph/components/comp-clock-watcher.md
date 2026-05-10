---
id: comp-clock-watcher
type: component
status: active
created: 2026-05-04T03:31:44.805213332Z
updated: 2026-05-06T07:23:05Z
edges:
- target: comp-nats-jetstream
  type: depends_on
- target: constraint-ntp-sync-required
  type: constrained_by
- target: feat-failure-handling
  type: used_by
- target: feat-substrate-services
  type: used_by
- target: principle-agent-first-bounded-supervision
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
---
Verifies NTP sync every 10min (§4.4.6, §21.3). Emits `clock.unsynced` if drift detected; ntfy-escalates per §2.12.

Rationale: clock skew is a debugging nightmare in distributed systems (§4.4.4). NTP-sync is a setup-script check (#7) and an ongoing reconciler check.

Crate `crates/jam-clock-watcher/` (bin).

Implementation status (2026-05-06): active MVP exists in `crates/jam-clock-watcher/`; it emits `clock.unsynced` to the journal when `timedatectl` reports an unsynchronized clock.
