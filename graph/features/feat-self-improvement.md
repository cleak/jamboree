---
id: feat-self-improvement
type: feature
status: draft
created: 2026-05-04T03:28:21.603071757Z
updated: 2026-05-04T15:58:42.315065785Z
owner: caleb
edges:
- target: comp-hermes-evolution-subsystem
  type: uses
- target: comp-jam-skills-monorepo-dir
  type: uses
- target: comp-jam-svc-evolve
  type: uses
- target: comp-jam-svc-knowledge
  type: uses
- target: comp-skill-suspicion-reconciler
  type: uses
- target: comp-skills-source-config
  type: uses
- target: dec-skills-direct-read-with-config
  type: depends_on
- target: dec-skills-in-monorepo-v1
  type: depends_on
- target: jamboree-v5
  type: child_of
- target: oq-runtime-skills-path
  type: has_question
- target: principle-adopt-subsystems-not-frameworks
  type: constrained_by
- target: principle-self-improvement-via-markdown-git-hermes
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
- target: task-skill-suspicion-reconciler-impl
  type: parent_of
- target: task-write-initial-skills
  type: parent_of
- target: the-manager
  type: serves
---
Skills as version-controlled markdown, three tiers of self-modification, evolution pipeline (§7).

Tier 1 — `record-learning` (low-friction): Maestro writes a structured skill note + emits a Tempyr `decision`/`finding`. No human gate. Reviewable via git history.
Tier 2 — `record-improvement-candidate`: flag a system-level change for human review. Never applied automatically.
Tier 3 — `propose-tool-change`: draft a tool surface change with rationale. Implemented by human, not Maestro.

Boundary between tiers 1 and 2: "does this change behavior, or does it inform behavior?" Skills inform; tool changes change.

Skill-suspicion (§7.4): `skill-suspicion-reconciler` queries `dead_end` corpus hourly; emits `skill.under-suspicion` when ≥3 entries within 7d tag a skill. Maestro sees event on next wake; decides to flag for evolution / deprecate / ignore. We don't auto-quarantine.

Hermes evolution pipeline (§7.3, §17.1): DSPy + GEPA over FTS5 session-store + Tempyr `dead_end` corpus. Output: candidate diff for human review. We never auto-promote.