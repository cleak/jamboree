---
id: comp-jam-svc-evolve
type: component
status: planned
created: 2026-05-04T03:39:37.279426230Z
updated: 2026-05-04T04:58:46.767391512Z
edges:
- target: api-request-skill-evolution
  type: exposes
- target: comp-events-toml-and-codegen
  type: depends_on
- target: comp-hermes-evolution-subsystem
  type: depends_on
- target: comp-jam-trace-crate
  type: depends_on
- target: comp-nats-jetstream
  type: depends_on
- target: comp-routing-manifest
  type: depends_on
- target: feat-maestro-tool-surface
  type: used_by
- target: feat-self-improvement
  type: used_by
- target: feat-skill-evolution-pipeline
  type: used_by
- target: feat-tool-services-out-of-process
  type: used_by
- target: principle-decoupled-processes-bus
  type: constrained_by
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
---
Skill evolution coordination. Subject prefix `tool.evolve.*`. Crate `crates/jam-svc-evolve/`.

Wraps the Hermes evolution pipeline subprocess (§4.4.7, §17.1). Triggered by:
- Periodic schedule (default weekly).
- `request-skill-evolution(skill-name)` Maestro tool call.
- `skill.under-suspicion` events.

Output: candidate skill diff written to `~/.jam/skills-evolution-candidates/<skill-name>.diff`. We never auto-promote.