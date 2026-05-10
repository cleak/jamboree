---
id: api-harness-adapter-contract
type: api_surface
status: stable
created: 2026-05-04T03:53:31.626404317Z
updated: 2026-05-06T22:20:25Z
edges:
- target: comp-harness-adapter-trait
  type: exposed_by
- target: feat-picker-layer-three-tier
  type: exposed_by
---
Implemented in `crates/jam-tools-core/src/contracts.rs` as `jam_tools_core::contracts::HarnessAdapter` (§4.5.1). Adding a new harness is one Rust struct + one config file + one harness skill markdown file.

Methods: `id`, `capabilities`, `spawn`, `inspect`, `enqueue_message`, `interrupt_with_message`, `full_stop`, `bootstrap_tempyr_journal`, `finalize_tempyr_journal`, `quota_state`, `current_version`, `current_checksum`.

`Capabilities` contains `features: Vec<HarnessCapability>`, `auth_modes`, `default_sandbox_backend`, and `min_version`. Harness features are `Interrupt`, `MessageQueue`, `WorktreeIsolation`, `ThinkingMode`, `SessionResume`, and `SessionStartHook`.

Capabilities drive routing decisions.
