---
id: comp-harness-adapter-trait
type: component
status: active
created: 2026-05-04T03:34:39.579319790Z
updated: 2026-05-06T22:20:25Z
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
Every harness implements the shared `HarnessAdapter` contract (§4.5.1, §19.3), now defined in `crates/jam-tools-core/src/contracts.rs`:

```rust
pub trait HarnessAdapter: Send + Sync {
    fn id(&self) -> HarnessId;
    fn capabilities(&self) -> Capabilities;
    fn spawn(&self, spec: SpawnSpec) -> ContractResult<PickerHandle>;
    fn inspect(&self, handle: &PickerHandle) -> ContractResult<PickerStatus>;
    fn enqueue_message(&self, handle: &PickerHandle, text: &str, trace_id: &str) -> ContractResult<MsgHandle>;
    fn interrupt_with_message(&self, handle: &PickerHandle, text: &str, trace_id: &str) -> ContractResult<MsgHandle>;
    fn full_stop(&self, handle: &PickerHandle, trace_id: &str) -> ContractResult<()>;
    fn bootstrap_tempyr_journal(&self, handle: &PickerHandle) -> ContractResult<()>;
    fn finalize_tempyr_journal(&self, handle: &PickerHandle) -> ContractResult<()>;
    fn quota_state(&self) -> ContractResult<HarnessQuotaState>;
    fn current_version(&self) -> ContractResult<String>;
    fn current_checksum(&self) -> ContractResult<String>;
}
```

`Capabilities` includes a `HarnessCapability` feature set plus `auth_modes`, `default_sandbox_backend`, and `min_version`.

Capabilities drive routing decisions (do not dispatch interrupt work to a harness lacking `HarnessCapability::Interrupt`; do not request long-context if backend is `local` and isolation is needed).

Service-specific implementations can live under `jam-svc-session`, but the trait surface lives in `jam-tools-core`.
