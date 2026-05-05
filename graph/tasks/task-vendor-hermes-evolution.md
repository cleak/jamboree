---
id: task-vendor-hermes-evolution
type: task
status: backlog
created: 2026-05-04T03:59:38.162154741Z
updated: 2026-05-04T04:13:28.075662743Z
edges:
- target: feat-skill-evolution-pipeline
  type: child_of
---
Phase 5 (§12). Vendor Hermes' evolution subsystem.

Per `comp-hermes-evolution-subsystem`, `feat-skill-evolution-pipeline`.

Boundary discipline: pipeline runs as subprocess. Reads a directory of skills + an eval data path, writes a diff. No Hermes module imports into main orchestrator code.

Acceptance: subprocess runs DSPy + GEPA optimization end-to-end; outputs candidate diff.