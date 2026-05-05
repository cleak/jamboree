---
id: task-skill-evolution-candidate-flow
type: task
status: backlog
created: 2026-05-04T03:59:46.830316720Z
updated: 2026-05-04T04:13:52.200032285Z
edges:
- target: feat-skill-evolution-pipeline
  type: child_of
---
Phase 5 (§12). Skill evolution candidate workflow: pipeline output → `~/.jam/skills-evolution-candidates/<name>.diff` → human review.

Per `feat-skill-evolution-pipeline`.

Acceptance: run skill evolution on a suspicious skill; verify a candidate diff appears and DSPy/GEPA optimization completes. Accept the candidate via `git commit`; verify next Picker task respects the new skill.