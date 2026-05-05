---
id: task-events-toml-initial-vocabulary
type: task
status: backlog
created: 2026-05-04T03:57:52.942761897Z
updated: 2026-05-04T04:08:32.667988442Z
edges:
- target: feat-substrate-services
  type: child_of
---
Phase 0 (§12). Populate `crates/jam-events/events.toml` with the initial event vocabulary: Picker lifecycle, PR/CI events, Maestro tool calls, patch events, quota state changes, journal/setup/meta events.

Per `comp-events-toml-and-codegen` and `dec-events-toml-manifest`.

Acceptance: events.toml round-trips through codegen; CI verifies generated files in sync.