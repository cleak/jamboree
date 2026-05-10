---
id: comp-stall-detector
type: component
status: active
created: 2026-05-04T03:31:39.812626874Z
updated: 2026-05-06T21:15:00Z
edges:
- target: comp-nats-jetstream
  type: depends_on
- target: feat-failure-handling
  type: used_by
- target: feat-substrate-services
  type: used_by
- target: principle-agent-first-bounded-supervision
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
---
Cheap deterministic process (§4.4.6). Subscribes `picker.*.output`, `picker.*.lifecycle`. Emits `picker.stalled`.

A Picker is stalled if any of:
- No new tokens emitted for `stall_token_idle_secs` (default 90s for active turns, 600s for idle waits).
- Same tool called with same arguments N+ times in a row (default N=3).
- Picker process running but its `world-snapshot` hasn't changed in `stall_progress_secs` (default 300s).

On stall detection, emits `picker.stalled` to bus. Maestro's wake-on-events brings it in to decide what to do (interrupt, kill, escalate). The detector itself takes no action — bounded deterministic supervision per §2.2.

Implementation note (2026-05-06): Phase 1 MVP is implemented as `crates/jam-stall-detector` with bin `jam-stall-detector`. It covers token-idle and repeated tool+arguments loops; the `world-snapshot` no-progress rule remains planned hardening.
