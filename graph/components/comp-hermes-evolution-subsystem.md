---
id: comp-hermes-evolution-subsystem
type: component
status: planned
created: 2026-05-04T03:39:51.046768901Z
updated: 2026-05-04T05:03:59.530716262Z
edges:
- target: comp-jam-svc-evolve
  type: depended_on_by
- target: dec-hermes-as-three-subsystems
  type: has_decision
- target: feat-self-improvement
  type: used_by
- target: feat-skill-evolution-pipeline
  type: used_by
- target: principle-adopt-subsystems-not-frameworks
  type: constrained_by
---
Hermes' DSPy + GEPA optimization scripts (`hermes_evolution/optimize.py`, `eval_data_loader.py`, GEPA reward shape) vendored as a subsystem (§17.1).

What we adapt:
- Eval source: ours is FTS5 session store + Tempyr `dead_end` corpus, not Hermes' chat logs.
- Output: candidate diff written to `~/.jam/skills-evolution-candidates/<name>.diff`, not auto-applied.
- Trigger: scheduled (weekly default) + on-demand (`request-skill-evolution`) + reactive (`skill.under-suspicion` events).

Boundary discipline (§17.1, §2.9): pipeline runs as subprocess. Reads a directory of skills + an eval data path, writes a diff. That's the whole interface. We don't import any Hermes modules into the main orchestrator code.

Lives at `evolution/` per §11.1.