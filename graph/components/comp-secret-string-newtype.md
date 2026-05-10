---
id: comp-secret-string-newtype
type: component
status: active
created: 2026-05-04T03:39:43.232434879Z
updated: 2026-05-06T21:15:00Z
edges:
- target: comp-file-secret-backend
  type: depended_on_by
- target: comp-jam-secrets
  type: depended_on_by
- target: comp-pass-secret-backend
  type: depended_on_by
- target: feat-tech-stack-hardening
  type: used_by
- target: principle-rust-trusted-core-python-agent-layer
  type: constrained_by
---
`SecretString` Rust newtype with `zeroize-on-drop` via `secrecy` crate (§11.3.2). Custom Debug/Display that prints `<redacted secret>`. No `Serialize` impl by default.

Logging discipline: journal-writer scans JSON payloads with regex patterns for known secret formats (Anthropic `sk-ant-...`, OpenAI `sk-...`, GitHub PAT `ghp_...`, etc.) and redacts before write. `bandit` (Python) and a custom clippy lint (Rust) catch direct format-string usage of `SecretString`.
