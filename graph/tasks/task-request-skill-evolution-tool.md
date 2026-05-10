---
id: task-request-skill-evolution-tool
type: task
status: done
created: 2026-05-04T03:59:49.726187074Z
updated: 2026-05-06T09:41:53Z
edges:
- target: feat-skill-evolution-pipeline
  type: child_of
---
Phase 5 (§12). `request-skill-evolution(skill-name)` Maestro tool.

Per `api-request-skill-evolution`.

Implemented the Maestro tool-surface contract and allowlist route. The
generated model `EvolveRequestSkillEvolutionRequest` validates `skill_name`,
optional `eval_source`, and optional `reason`, and `MaestroToolRegistry`
routes `request-skill-evolution` to
`tool.evolve.request-skill-evolution`. The actual `jam-svc-evolve`
coordinator remains tracked separately by `task-jam-svc-evolve-coordinator`.
