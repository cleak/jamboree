---
id: comp-jam-secrets
type: component
status: planned
created: 2026-05-04T03:39:39.635246686Z
updated: 2026-05-04T05:00:36.963904484Z
edges:
- target: api-secret-backend-contract
  type: exposes
- target: comp-file-secret-backend
  type: depended_on_by
- target: comp-jam-cli-binary
  type: depended_on_by
- target: comp-jam-svc-session
  type: depended_on_by
- target: comp-pass-secret-backend
  type: depended_on_by
- target: comp-secret-string-newtype
  type: depends_on
- target: feat-tech-stack-hardening
  type: used_by
---
`SecretBackend` trait + `pass` and file backends (§11.3.2). Crate `crates/jam-secrets/`.

```rust
pub trait SecretBackend: Send + Sync {
    fn get(&self, key: &SecretKey) -> Result<SecretString>;
    fn list_keys(&self) -> Result<Vec<SecretKey>>;
}
pub struct SecretKey(String);  // newtype: prevents key from being logged accidentally
pub struct SecretString {
    inner: SecretBox<String>,  // zeroize-on-drop via secrecy crate
}
// Custom Debug/Display: "<redacted secret>"
// No Serialize impl by default.
pub struct PassBackend { prefix: String }
pub struct FileBackend { path: PathBuf }
```

`secret_backend.get_for_harness(harness_id)` enforces per-harness allowlist — Codex CLI Picker doesn't get the DeepSeek key; a docs-summary Picker doesn't get the GitHub PAT.

Three layers of protection (§11.3.2): storage (encrypted-on-disk via pass+GPG, or chmod 600 file fallback), in-memory (zeroize-on-drop, redacted Debug/Display), logging discipline (regex redaction at journal write time, plus bandit/clippy lints).

Linux-only deployment per `principle-linux-only-deployment`.