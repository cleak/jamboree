---
id: api-compute-readiness
type: api_surface
status: draft
created: 2026-05-04T03:51:49.084708401Z
updated: 2026-05-04T04:53:09.047003096Z
edges:
- target: comp-jam-svc-observe
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`compute-readiness(task-id)` → `ReadinessVerdict` (§5.1):
- `NotReady{blockers}`
- `Ready`
- `ReadyWithWarnings{warnings}`

Reads from world-snapshot. Surfaces branch staleness, Tempyr index drift, CI status, review artifact open-counts.

Maestro disagrees and overrides whenever justified — `compute-readiness` is signal, not policy.