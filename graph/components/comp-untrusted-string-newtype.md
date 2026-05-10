---
id: comp-untrusted-string-newtype
type: component
status: active
created: 2026-05-04T03:39:29.653623290Z
updated: 2026-05-06T09:22:22Z
edges:
- target: comp-pyproject-tooling
  type: depended_on_by
- target: feat-sandboxing-profile-x-backend
  type: used_by
- target: feat-tech-stack-hardening
  type: used_by
- target: insight-untrusted-newtype-prevents-injection
  type: relates_to
- target: principle-untrusted-content-cannot-issue-commands
  type: constrained_by
---
`Untrusted<String>` (Rust) and `Untrusted` NewType (Python) discipline (§11.2.4, §6.1).

Rust:
```rust
pub struct Untrusted<T>(T);
impl Untrusted<String> {
    pub fn into_inner_for_display(self) -> String { ... }
    // No automatic Deref or Display impl — explicit unwrapping required.
}
```

Python:
```python
from typing import NewType
Untrusted = NewType("Untrusted", str)
```

pyright/clippy catch direct format-string usage of `Untrusted` into shell commands or system prompts.

Sources of untrusted content (§6.1):
- PR descriptions, review comments, CI logs.
- Web search results, web extract results, MCP responses.
- Tempyr node bodies if Maestro authored (treat as Untrusted by default).
- Email/chat content (when MCP integrations enabled).

Tools that take `Untrusted[str]` know to never format into a shell command, never put directly in a system prompt, never log without redaction.

Implementation note (2026-05-06): Rust support lives in `crates/jam-untrusted`; Python support lives in `maestro/src/jam_maestro/untrusted.py`. Rust uses the absence of `Display`/`Deref` as a stronger compile-time guard than a lint for direct formatting. Python tool boundaries should require `TrustedText`, with conversion from `Untrusted` only through `trust_after_review`.
