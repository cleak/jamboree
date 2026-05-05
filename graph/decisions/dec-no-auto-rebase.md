---
id: dec-no-auto-rebase
type: decision
status: decided
created: 2026-05-04T03:46:30.909332926Z
updated: 2026-05-04T04:40:15.268440889Z
edges:
- target: feat-maestro-tool-surface
  type: depended_on_by
---
**No auto-rebase** (§4.2.3, §6.11, §5.9). The Maestro sees `branch_staleness` and decides whether to rebase, merge, or ignore — but the orchestrator never auto-rebases.

Why: auto-rebase produces silent corruption when the Picker has uncommitted state or when conflicts are subtle. The Maestro plus a clear staleness signal is preferable to mechanical merging.

Captured as `principle-no-auto-rebase`.