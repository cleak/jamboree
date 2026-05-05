---
id: comp-sandbox-backend-trait
type: component
status: planned
created: 2026-05-04T03:39:22.084422435Z
updated: 2026-05-04T05:00:27.515384971Z
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
The `SandboxBackend` trait (§6.2, §19.4):

```rust
pub trait SandboxBackend: Send + Sync {
    fn id(&self) -> SandboxBackendId;
    fn prepare(&self, spec: &SpawnSpec) -> Result<SandboxedEnvironment>;
    fn launch(&self, env: &SandboxedEnvironment, cmd: Command) -> Result<Child>;
    fn cleanup(&self, env: &SandboxedEnvironment) -> Result<()>;
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