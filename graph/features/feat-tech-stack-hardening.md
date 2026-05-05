---
id: feat-tech-stack-hardening
type: feature
status: draft
created: 2026-05-04T03:28:22.438481137Z
updated: 2026-05-04T05:05:23.644817407Z
owner: caleb
edges:
- target: api-secret-backend-contract
  type: exposes
- target: comp-file-secret-backend
  type: uses
- target: comp-jam-secrets
  type: uses
- target: comp-pass-secret-backend
  type: uses
- target: comp-pre-commit-hooks
  type: uses
- target: comp-pyproject-tooling
  type: uses
- target: comp-secret-string-newtype
  type: uses
- target: comp-untrusted-string-newtype
  type: uses
- target: constraint-wsl-pinentry-curses
  type: constrained_by
- target: dec-pass-and-gpg-for-secrets
  type: depends_on
- target: insight-untrusted-newtype-prevents-injection
  type: informed_by
- target: jamboree-v5
  type: child_of
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-rust-trusted-core-python-agent-layer
  type: constrained_by
- target: principle-untrusted-content-cannot-issue-commands
  type: constrained_by
- target: task-untrusted-newtype-and-lints
  type: parent_of
---
Rust + Python tooling and discipline (§11):

Python (§11.2):
- `uv` for package mgmt, `ruff` with `select = ["ALL"]`, `pyright` strict.
- All LLM tool calls validated through Pydantic (§11.2.2).
- No `eval`, no `exec`, no `shell=True`. `bandit` enforces.
- `Untrusted<str>` NewType discipline (§11.2.4).
- Hypothesis property tests (§11.2.5).
- Rust ↔ Python type stub generation (§11.2.6).
- Pre-commit hooks: ruff + format + pyright + gitleaks + events-codegen-check + schema-export-check (§11.2.7).
- CI matrix on PRs (§11.2.8).

Rust:
- `cargo clippy --workspace --all-targets -- -D warnings`, `cargo deny`, `cargo audit`, `cargo fmt --check`.
- `schemars` derive for type-to-JSON-schema.
- `secrecy` crate for `SecretString` zeroize-on-drop.

Secrets: `pass` primary backend, file fallback (§11.3). `~/.jam/config/secrets.toml` declares `backend = "pass" | "file" | "env"` and `pass-prefix = "jam"`.