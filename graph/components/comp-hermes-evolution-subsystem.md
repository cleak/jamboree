---
id: comp-hermes-evolution-subsystem
type: component
status: active
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

Implementation note (2026-05-06): the official subsystem is `NousResearch/hermes-agent-self-evolution`, vendored at commit `4693c8f0eed21e39f065c6f38d98d2a403a04095` under `evolution/hermes-agent-self-evolution/`. Jamboree's local adapter is `evolution/jamboree_evolve_skill.py`; it keeps Hermes code behind a subprocess call and emits reviewable candidate diffs instead of applying skill changes. `scripts/smoke-hermes-evolution-vendor.sh` validates upstream tests and dry-run subprocess wiring without making an LLM call.
