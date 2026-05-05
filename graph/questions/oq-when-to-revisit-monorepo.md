---
id: oq-when-to-revisit-monorepo
type: open_question
status: open
created: 2026-05-04T03:47:38.899279172Z
updated: 2026-05-04T03:47:38.899279699Z
---
**When to split the monorepo** (layout.md §When to revisit).

Concrete triggers:
- Spec becomes a public reference document (users want to read without seeing source).
- A second implementation in a different language emerges (multi-language ports).
- Build/CI on the whole repo gets slow (>5 min for a no-op).
- Multiple contributors with different access needs to spec vs. impl.

Until one is true, monorepo wins. Revisit if any trigger fires.