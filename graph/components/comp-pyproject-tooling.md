---
id: comp-pyproject-tooling
type: component
status: active
created: 2026-05-04T03:39:48.116714014Z
updated: 2026-05-06T21:15:00Z
edges:
- target: comp-untrusted-string-newtype
  type: depends_on
- target: feat-tech-stack-hardening
  type: used_by
---
Python tooling stack (§11.2.1) at `maestro/pyproject.toml`:
- `uv` for package management (faster than pip/poetry).
- `ruff` with `select = ["ALL"]` (aggressive; cost of false positives < cost of unattended overnight failures).
- `pyright` strict mode forces typed dict usage through Pydantic models.
- Pytest with `--strict-markers --strict-config` and markers: `slow`, `integration`, `live-llm`.

Test discipline (§11.2.5) — `maestro/tests/` split into `unit/`, `integration/`, `live-llm/`, `property/`. Hypothesis property tests on path-safety invariants, workspace-key sanitization, world-snapshot freshness logic, NATS-arrival-order semantics, trace propagation completeness.
