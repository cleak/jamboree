---
id: task-jam-secrets-pass-and-file-backends
type: task
status: done
created: 2026-05-04T03:57:58.038234992Z
updated: 2026-05-06T02:51:58.945332121Z
edges:
- target: feat-substrate-services
  type: child_of
---
Phase 0 (§12). Implement `crates/jam-secrets/` with `pass` and file backends. `SecretString` newtype with zeroize-on-drop via `secrecy` crate.

Per `comp-jam-secrets`, `comp-pass-secret-backend`, `comp-file-secret-backend`, `comp-secret-string-newtype`.

Acceptance: `SecretBackend::get(SecretKey)` returns `SecretString` from pass or file fallback; Debug/Display redacts; per-harness allowlist via `get_for_harness` works.