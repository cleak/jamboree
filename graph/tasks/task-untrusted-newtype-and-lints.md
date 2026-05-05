---
id: task-untrusted-newtype-and-lints
type: task
status: backlog
created: 2026-05-04T03:58:57.593619793Z
updated: 2026-05-04T04:11:32.502538403Z
edges:
- target: feat-tech-stack-hardening
  type: child_of
---
Phase 2 (§12). `Untrusted<String>` newtype in Rust; `Untrusted` NewType in Python; lint rules in CI.

Per `comp-untrusted-string-newtype`, `principle-untrusted-content-cannot-issue-commands`, `insight-untrusted-newtype-prevents-injection`.

Acceptance: Rust clippy lint catches `format!("{}", untrusted)` in non-trace crates; Python pyright catches `send_to_picker(picker_id, untrusted_comment_body)`.