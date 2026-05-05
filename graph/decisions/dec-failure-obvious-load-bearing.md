---
id: dec-failure-obvious-load-bearing
type: decision
status: decided
created: 2026-05-04T03:46:11.065450157Z
updated: 2026-05-04T04:37:39.570273851Z
edges:
- target: feat-failure-handling
  type: depended_on_by
---
**Failure-obvious as a design principle** (§v5 changes #6, §2.12). Every component refuses to operate silently, surfaces the specific reason, offers remediation hints when possible. Applied across setup, runtime, recovery.

Why: silent degradation produces bad outputs that look fine; loud failure gets fixed.

The worst possible outcome is "the orchestrator was running fine all weekend but every PR it produced was subtly broken." This principle prevents that class of outcome at the cost of slightly more aggressive crashes during development.