---
id: api-secret-backend-contract
type: api_surface
status: draft
created: 2026-05-04T03:53:41.173938945Z
updated: 2026-05-04T05:00:36.963904845Z
edges:
- target: comp-jam-secrets
  type: exposed_by
- target: feat-tech-stack-hardening
  type: exposed_by
---
The `SecretBackend` trait (§11.3.2):

```rust
pub trait SecretBackend: Send + Sync {
    fn get(&self, key: &SecretKey) -> Result<SecretString>;
    fn list_keys(&self) -> Result<Vec<SecretKey>>;
}
```

Implementations: `PassBackend`, `FileBackend`. Plus per-harness allowlist via `secret_backend.get_for_harness(harness_id)`.