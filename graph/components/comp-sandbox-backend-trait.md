---
id: comp-sandbox-backend-trait
type: component
status: active
created: 2026-05-04T03:39:22.084422435Z
updated: 2026-05-06T22:20:25Z
edges:
- target: api-sandbox-backend-contract
  type: exposes
- target: comp-docker-sandbox-backend
  type: depended_on_by
- target: comp-jam-svc-session
  type: depended_on_by
- target: comp-local-sandbox-backend
  type: depended_on_by
- target: comp-modal-sandbox-backend
  type: depended_on_by
- target: comp-ssh-sandbox-backend
  type: depended_on_by
- target: feat-sandboxing-profile-x-backend
  type: used_by
---
The shared `SandboxBackend` contract (§6.2, §19.4), now defined in `crates/jam-tools-core/src/contracts.rs`:

```rust
pub trait SandboxBackend: Send + Sync {
    fn id(&self) -> SandboxBackendId;
    fn prepare(&self, spec: &SpawnSpec) -> ContractResult<SandboxedEnvironment>;
    fn launch(&self, env: &SandboxedEnvironment, cmd: Command) -> ContractResult<Child>;
    fn cleanup(&self, env: &SandboxedEnvironment) -> ContractResult<()>;
}

pub struct SandboxedEnvironment {
    pub effective_path: PathBuf,
    pub effective_env: HashMap<String, String>,
    pub network_policy: NetworkPolicy,
    pub resource_limits: ResourceLimits,
    pub teardown_token: TeardownToken,
}
```

Implementations: `LocalBackend`, `DockerBackend`, `SshBackend`, `ModalBackend`. Each one knows how to apply the profile to its environment.
