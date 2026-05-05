---
id: feat-skill-evolution-pipeline
type: feature
status: draft
created: 2026-05-04T03:28:26.251438428Z
updated: 2026-05-04T04:25:37.369264240Z
owner: caleb
edges:
- target: comp-hermes-evolution-subsystem
  type: uses
- target: comp-jam-svc-evolve
  type: uses
- target: comp-skill-suspicion-reconciler
  type: uses
- target: jamboree-v5
  type: child_of
- target: principle-adopt-subsystems-not-frameworks
  type: constrained_by
- target: principle-self-improvement-via-markdown-git-hermes
  type: constrained_by
- target: task-jam-svc-evolve-coordinator
  type: parent_of
- target: task-request-skill-evolution-tool
  type: parent_of
- target: task-skill-evolution-candidate-flow
  type: parent_of
- target: task-vendor-hermes-evolution
  type: parent_of
---
Wraps `hermes-agent-self-evolution` (DSPy + GEPA) as a subsystem (§7.3, §17.1). Runs as a separate Python process. Triggered by:

- Periodic schedule (default weekly).
- `request-skill-evolution(skill-name)` tool call from Maestro.
- `skill.under-suspicion` events when a skill accumulates ≥3 `dead_end` entries within 7d (§22.6).

Pipeline steps: select skills for evaluation based on usage frequency and observed disagreement → run DSPy optimization with GEPA → produce candidate diff → write to `~/.jam/skills-evolution-candidates/<skill-name>.diff` → human reviews via `git commit` on skills repo, accepts or rejects.

Boundary discipline (§17.1): pipeline runs as subprocess. It reads a directory of skills + an eval data path, writes a diff. That's the whole interface. No Hermes module imports into main orchestrator code.

We never auto-promote evolved skills.