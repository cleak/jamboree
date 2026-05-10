---
id: task-skill-evolution-candidate-flow
type: task
status: blocked
created: 2026-05-04T03:59:46.830316720Z
updated: 2026-05-06T16:40:06.692351837Z
edges:
- target: feat-skill-evolution-pipeline
  type: child_of
---
Phase 5 (§12). Skill evolution candidate workflow: pipeline output → `~/.jam/skills-evolution-candidates/<name>.diff` → human review.

Per `feat-skill-evolution-pipeline`.

Acceptance: run skill evolution on a suspicious skill; verify a candidate diff appears and DSPy/GEPA optimization completes. Accept the candidate via `git commit`; verify next Picker task respects the new skill.

Blocked note (2026-05-06): this end-to-end candidate workflow depends on the actual DSPy/GEPA optimizer and the `jam-svc-evolve` coordinator. The Hermes subprocess is now vendored and dry-run verified, but real optimization remains blocked until an LLM credential is seeded for DSPy/GEPA. To finish acceptance, run the adapter on a suspicious skill until it writes a candidate diff, wire/verify the coordinator path, then do the human review/commit and follow-up Picker verification.
