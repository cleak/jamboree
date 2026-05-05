---
id: principle-no-auto-merge
type: constraint
status: active
created: 2026-05-04T03:23:49.823616701Z
updated: 2026-05-04T04:12:30.009319988Z
edges:
- target: feat-maestro-tool-surface
  type: constrains
- target: feat-tempyr-consistency-model
  type: constrains
---
Merge is the only hard human gate (§1 Non-goals; §5.4; §5.9). No `merge-pr` tool exists in the Maestro tool surface. The closest tool is `request-human-merge`, which writes a notification and waits for the human to merge via the GitHub UI.

This is a corollary of §2.3 (structure lives in tools, not policy) — the invariant lives in the absence of a tool, not in a runtime guard.