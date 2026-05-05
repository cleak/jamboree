---
id: comp-pre-commit-hooks
type: component
status: planned
created: 2026-05-04T03:39:48.534157037Z
updated: 2026-05-04T04:52:07.777354257Z
edges:
- target: comp-events-toml-and-codegen
  type: depends_on
- target: feat-tech-stack-hardening
  type: used_by
---
`.pre-commit-config.yaml` (§11.2.7):
- `ruff` (with `--fix`)
- `ruff-format`
- `pyright` (local hook via `uv run pyright`)
- `gitleaks protect --staged --redact`
- `events-codegen-check` — `python tools/events-codegen.py --check` (events.toml in sync with generated files)
- `schema-export-check` — `cargo run --bin schema-export -- --check` (JSON schemas in sync with Rust types)

CI matrix (§11.2.8) on PRs runs the same plus `pytest tests/unit tests/integration -q`, `pip-audit`, `bandit -r src`, `gitleaks detect --no-git`, `cargo test --workspace --locked`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`, `cargo audit`, `cargo deny check`. `live-llm` and `slow` markers excluded from PR CI; run on nightly.