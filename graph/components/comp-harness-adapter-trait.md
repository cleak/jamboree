---
id: comp-harness-adapter-trait
type: component
status: planned
created: 2026-05-04T03:34:39.579319790Z
updated: 2026-05-04T04:59:59.823141026Z
edges:
- target: api-harness-adapter-contract
  type: exposes
- target: comp-aider-adapter
  type: depended_on_by
- target: comp-claude-code-adapter
  type: depended_on_by
- target: comp-codex-cli-adapter
  type: depended_on_by
- target: comp-cursor-cli-adapter
  type: depended_on_by
- target: comp-jam-svc-session
  type: depended_on_by
- target: comp-opencode-deepseek-adapter
  type: depended_on_by
- target: feat-messaging-three-modes
  type: used_by
- target: feat-picker-layer-three-tier
  type: used_by
---
Every harness implements `HarnessAdapter` (§4.5.1, §19.3):

```rust
pub trait HarnessAdapter: Send + Sync {
    fn id(&self) -> HarnessId;
    fn capabilities(&self) -> Capabilities;
    fn spawn(&self, spec: SpawnSpec) -> Result<PickerHandle>;
    fn inspect(&self, handle: &PickerHandle) -> Result<PickerStatus>;
    fn enqueue_message(&self, handle, text, trace_id) -> Result<MsgHandle>;
    fn interrupt_with_message(&self, handle, text, trace_id) -> Result<MsgHandle>;
    fn full_stop(&self, handle, trace_id) -> Result<()>;
    fn bootstrap_tempyr_journal(&self, handle) -> Result<()>;
    fn finalize_tempyr_journal(&self, handle) -> Result<()>;
    fn quota_state(&self) -> Result<HarnessQuotaState>;
    fn current_version(&self) -> Result<String>;
    fn current_checksum(&self) -> Result<String>;
}
```

`Capabilities` includes `supports_interrupt`, `supports_message_queue`, `supports_worktree_isolation`, `supports_thinking_mode`, `supports_session_resume`, `supports_session_start_hook`, `auth_modes`, `default_sandbox_backend`, `min_version`.

Capabilities drive routing decisions (don't dispatch to harness lacking `supports_interrupt`; don't request long-context if backend is `local` and isolation needed).

Lives in `crates/jam-svc-session/src/harness.rs`.