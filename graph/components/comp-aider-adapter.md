---
id: comp-aider-adapter
type: component
status: planned
created: 2026-05-04T03:34:42.326237679Z
updated: 2026-05-04T04:42:25.439921169Z
edges:
- target: comp-harness-adapter-trait
  type: depends_on
- target: feat-picker-layer-three-tier
  type: used_by
---
Specialized harness (§4.5.4). Loaded conditionally per project — most projects don't need them. The harness adapter trait makes adding one a matter of writing one Rust struct that implements `HarnessAdapter`.

Deferred until a specific use case demands it.