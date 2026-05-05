---
id: dec-no-auto-merge-no-merge-pr-tool
type: decision
status: decided
created: 2026-05-04T03:46:29.385019114Z
updated: 2026-05-04T04:40:03.612456665Z
edges:
- target: feat-maestro-tool-surface
  type: depended_on_by
---
**No `merge-pr` tool exists** (§2.3, §5.4, §5.9). Merge requires human action through GitHub. The closest tool is `request-human-merge`, which writes a notification and waits.

Why: invariant lives in tool absence (`principle-structure-in-tools-not-policy`). Policy-checks-on-the-Maestro are easily bypassed by a creative agent; tool-shape invariants are mechanically impossible to bypass.

This is the canonical example of "structure in tools, not policy."