---
id: comp-time-and-clock
type: component
status: planned
created: 2026-05-04T03:31:38.467137801Z
updated: 2026-05-04T04:28:24.640337891Z
edges:
- target: constraint-ntp-sync-required
  type: constrained_by
- target: feat-substrate-services
  type: used_by
---
Rules in order (§4.4.4):

1. All timestamps are UTC, RFC 3339 with nanosecond precision.
2. Sourced from `chrono::Utc::now()` (Rust) or `datetime.now(timezone.utc)` (Python) at the producing service.
3. Within a NATS subject, ordering is by NATS sequence number (or `journal_seq`), not by timestamp.
4. Across subjects (or for cross-service "what happened first"), ordering is by timestamp with NATS sequence as tiebreaker.
5. All systems involved (orchestrator host, SSH backends, Modal containers) MUST be NTP-synced. Supervisor verifies clock skew at startup; warns if drift > 1s.
6. SSH and Modal backends emit events with their own clock; the orchestrator records both `producing_clock_at` (producer's UTC) and `received_at` (NATS ingestion UTC). Reconciler uses `received_at` when `producing_clock_at` would create paradoxes.

Setup script verifies `timedatectl show -p NTPSynchronized` returns `yes`.