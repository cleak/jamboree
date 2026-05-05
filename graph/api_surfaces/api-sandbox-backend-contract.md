---
id: api-sandbox-backend-contract
type: api_surface
status: draft
created: 2026-05-04T03:53:39.069210993Z
updated: 2026-05-04T05:00:27.515385281Z
edges:
- target: comp-sandbox-backend-trait
  type: exposed_by
- target: feat-sandboxing-profile-x-backend
  type: exposed_by
---
The `SandboxBackend` trait (§19.4, §6.2):

```rust
pub trait SandboxBackend: Send + Sync {
    fn id(&self) -> SandboxBackendId;
    fn prepare(&self, spec: &SpawnSpec) -> Result<SandboxedEnvironment>;
    fn launch(&self, env: &SandboxedEnvironment, cmd: Command) -> Result<Child>;
    fn cleanup(&self, env: &SandboxedEnvironment) -> Result<()>;
}
```

Implementations: Local, Docker, Ssh, Modal. Each one knows how to apply the profile to its environment.