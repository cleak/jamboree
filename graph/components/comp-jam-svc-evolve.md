---
id: comp-jam-svc-evolve
type: component
status: active
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

Implementation note (2026-05-06): `crates/jam-svc-evolve` now serves traced `tool.evolve.request-skill-evolution` requests. It resolves a requested skill under `JAM_EVOLVE_SKILLS_DIR`, invokes `evolution/jamboree_evolve_skill.py` through the vendored Hermes package as a subprocess, and returns either a candidate diff path or a typed failure. `scripts/smoke-evolve-coordinator.sh` passed with live NATS and `JAM_EVOLVE_DRY_RUN=true`, proving request/reply trace handling and subprocess wiring without spending LLM tokens. Real candidate generation remains blocked on the DSPy/GEPA model credential tracked by `task-vendor-hermes-evolution`.
