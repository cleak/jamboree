---
id: principle-no-auto-rebase
type: constraint
status: active
created: 2026-05-04T03:23:50.016243157Z
updated: 2026-05-04T04:12:37.119500955Z
edges:
- target: feat-maestro-tool-surface
  type: constrains
---
We never auto-rebase. The Maestro sees `branch_staleness` (§4.2.3, §6.11) and decides whether to rebase, merge, or ignore — but the orchestrator never auto-rebases.

*Why:* auto-rebase produces silent corruption when the Picker has uncommitted state or when conflicts are subtle. The Maestro plus a clear staleness signal is preferable to mechanical merging.

A specific consequence: `compute-readiness` flags stale branches and the Maestro reasons about them, but the tool surface contains no `auto-rebase` tool — once again the invariant is tool-shape (§2.3).