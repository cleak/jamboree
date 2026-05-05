---
id: api-harness-adapter-contract
type: api_surface
status: draft
created: 2026-05-04T03:53:31.626404317Z
updated: 2026-05-04T04:59:59.823141521Z
edges:
- target: comp-harness-adapter-trait
  type: exposed_by
- target: feat-picker-layer-three-tier
  type: exposed_by
---
The `HarnessAdapter` trait is the contract for harness integration (§4.5.1). Adding a new harness is one Rust struct + one config file + one harness skill markdown file.

Methods: `id`, `capabilities`, `spawn`, `inspect`, `enqueue_message`, `interrupt_with_message`, `full_stop`, `bootstrap_tempyr_journal`, `finalize_tempyr_journal`, `quota_state`, `current_version`, `current_checksum`.

`Capabilities`: `supports_interrupt`, `supports_message_queue`, `supports_worktree_isolation`, `supports_thinking_mode`, `supports_session_resume`, `supports_session_start_hook`, `auth_modes`, `default_sandbox_backend`, `min_version`.

Capabilities drive routing decisions.