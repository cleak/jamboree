---
id: constraint-ntp-sync-required
type: constraint
status: active
created: 2026-05-04T03:23:51.112075978Z
updated: 2026-05-04T04:28:34.736090056Z
edges:
- target: comp-clock-watcher
  type: constrains
- target: comp-time-and-clock
  type: constrains
- target: feat-live-update-flows
  type: constrains
- target: feat-substrate-services
  type: constrains
---
All systems involved (orchestrator host, SSH backends, Modal containers) MUST be NTP-synced (§4.4.4). The supervisor verifies clock skew at startup and warns if drift > 1s. The setup script (§11.4 check #7) refuses to install if `timedatectl show -p NTPSynchronized` does not return `yes`.

`clock-watcher` reconciler runs every 10 minutes and emits `clock.unsynced` if drift returns; ntfy-escalates per §2.12.

*Why:* clock skew is a debugging nightmare in distributed systems. Pinning UTC at producer + sequence-number tiebreaker is the minimum hygiene that lets traces be reconstructed reliably across services. NTP-sync extends this property to cross-machine setups.