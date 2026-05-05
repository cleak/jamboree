---
id: dec-monorepo
type: decision
status: decided
created: 2026-05-04T03:46:45.018285270Z
updated: 2026-05-04T05:04:27.559574266Z
edges:
- target: comp-monorepo-tree
  type: decision_for
---
**Monorepo for spec + scripts + Rust crates + Python Maestro + UI** (layout.md).

Why: solo dev, no audience separation, no independent versioning, no security-context split at source level (runtime executes as `maestro`/`picker`, but source-of-truth is always at `~caleb/jamboree/`).

Reversibility: splitting later via `git filter-repo --subdirectory-filter docs/` extracts a subtree with full history. Merging two repos back is messier — so monorepo is the lower-risk default.

Source vs. runtime bridge: `jam patch apply` (§21.6).