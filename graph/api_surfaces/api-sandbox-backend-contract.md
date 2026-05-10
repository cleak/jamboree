---
id: api-sandbox-backend-contract
type: api_surface
status: stable
created: 2026-05-04T03:53:39.069210993Z
updated: 2026-05-06T22:20:25Z
edges:
- target: comp-sandbox-backend-trait
  type: exposed_by
- target: feat-sandboxing-profile-x-backend
  type: exposed_by
---
Implemented in `crates/jam-tools-core/src/contracts.rs` as `jam_tools_core::contracts::SandboxBackend` (§19.4, §6.2):

```rust
pub trait SandboxBackend: Send + Sync {
    fn id(&self) -> SandboxBackendId;
    fn prepare(&self, spec: &SpawnSpec) -> ContractResult<SandboxedEnvironment>;
    fn launch(&self, env: &SandboxedEnvironment, cmd: Command) -> ContractResult<Child>;
    fn cleanup(&self, env: &SandboxedEnvironment) -> ContractResult<()>;
}
```

Implementations: Local, Docker, Ssh, Modal. Each one knows how to apply the profile to its environment.
