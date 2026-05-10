---
id: comp-file-secret-backend
type: component
status: active
created: 2026-05-04T03:39:42.022961506Z
updated: 2026-05-06T21:15:00Z
edges:
- target: comp-jam-secrets
  type: depends_on
- target: comp-secret-string-newtype
  type: depends_on
- target: feat-tech-stack-hardening
  type: used_by
---
File-based fallback secret backend (§11.3). `~/.jam/config/secrets.toml` (chmod 600, owned by user).

WSL gotcha: ensure file is on Linux filesystem (verified by §6.6 Invariant 4 / `principle-native-fs-only`).
