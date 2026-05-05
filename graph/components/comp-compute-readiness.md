---
id: comp-compute-readiness
type: component
status: planned
created: 2026-05-04T03:31:32.788136429Z
updated: 2026-05-04T04:05:55.604517249Z
edges:
- target: feat-observation-tool-service
  type: used_by
---
`compute-readiness(task-id)` returns a `ReadinessVerdict`:
- `NotReady{blockers}`
- `Ready`
- `ReadyWithWarnings{warnings}`

Reads from world-snapshot. Surfaces branch staleness (§6.11), Tempyr index drift (§4.6.4), CI status, review artifact open-counts.

The Maestro disagrees and overrides whenever justified — `compute-readiness` is signal, not policy (§2.1).

`list-blockers(task-id)` returns `Vec<Blocker>` directly when readiness is `NotReady`.