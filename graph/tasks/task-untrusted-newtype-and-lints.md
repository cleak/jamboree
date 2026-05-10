---
id: task-untrusted-newtype-and-lints
type: task
status: done
created: 2026-05-04T03:58:57.593619793Z
updated: 2026-05-06T09:22:22Z
edges:
- target: feat-tech-stack-hardening
  type: child_of
---
Phase 2 (§12). `Untrusted<String>` newtype in Rust; `Untrusted` NewType in Python; lint rules in CI.

Per `comp-untrusted-string-newtype`, `principle-untrusted-content-cannot-issue-commands`, `insight-untrusted-newtype-prevents-injection`.

Acceptance: Rust clippy lint catches `format!("{}", untrusted)` in non-trace crates; Python pyright catches `send_to_picker(picker_id, untrusted_comment_body)`.

Implementation note (2026-05-06): added Rust crate `crates/jam-untrusted` with `Untrusted<T>`. It intentionally implements neither `Display` nor `Deref`, so direct formatting such as `format!("{}", untrusted)` fails at compile time; doctest compile-fail coverage locks this behavior. The wrapper exposes only explicit boundary methods: `as_ref_for_analysis`, `into_inner_after_review`, and `map`.

Python note (2026-05-06): added `jam_maestro.untrusted` with `Untrusted` and `TrustedText` `NewType` markers plus `mark_untrusted` and `trust_after_review`. Pyright coverage uses a negative fixture proving `send_to_picker(..., untrusted_comment_body)` is rejected when the tool boundary requires `TrustedText`.

Verification (2026-05-06): `cargo test -p jam-untrusted` passed unit tests plus compile-fail doctests; `uv run pytest tests/unit/test_untrusted.py`, `uv run pyright`, and `uv run ruff check` passed in `maestro/`.
