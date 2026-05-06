---
id: task-events-toml-initial-vocabulary
type: task
status: done
created: 2026-05-04T03:57:52.942761897Z
updated: 2026-05-06T02:51:41.179152710Z
edges:
- target: feat-substrate-services
  type: child_of
---
Phase 0 (§12). Populate `crates/jam-events/events.toml` with the initial event vocabulary: Picker lifecycle, PR/CI events, Maestro tool calls, patch events, quota state changes, journal/setup/meta events.

Per `comp-events-toml-and-codegen` and `dec-events-toml-manifest`.

Acceptance: events.toml round-trips through codegen; CI verifies generated files in sync.