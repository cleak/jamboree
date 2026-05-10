---
id: task-clock-watcher-impl
type: task
status: done
created: 2026-05-04T04:01:39.971263319Z
updated: 2026-05-06T07:23:05Z
---
Implement `clock-watcher` reconciler — verifies NTP sync every 10min; emits `clock.unsynced` on drift.

Per `comp-clock-watcher`, `constraint-ntp-sync-required`.

Implementation note (2026-05-06): `crates/jam-clock-watcher` implements the watcher daemon. It runs `timedatectl show -p NTPSynchronized -p SystemClockSynchronized`, defaults to a 600s cadence, exits loudly if the command or output shape is invalid, and emits traced `journal.clock.unsynced` when the host reports unsynchronized time. Unsynced status is represented as `drift_ms = threshold + 1` because `timedatectl` reports sync state rather than a precise offset.

Live smoke (2026-05-06): temporary NATS root `/tmp/jam-clock-watcher-smoke-jLU4CQ` used a fake `timedatectl` returning `NTPSynchronized=no`. `jam-clock-watcher --once` emitted trace `01KQY2NSDFD5GQAWJVWQJZEJRM`, and `jam-nats-bridge` wrote one `journal.clock.jsonl` entry with `event_type=clock.unsynced` and `drift_ms=1001`.
