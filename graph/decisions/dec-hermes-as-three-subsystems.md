---
id: dec-hermes-as-three-subsystems
type: decision
status: decided
created: 2026-05-04T03:46:41.826281776Z
updated: 2026-05-04T05:04:17.978786537Z
edges:
- target: comp-docker-sandbox-backend
  type: decision_for
- target: comp-hermes-evolution-subsystem
  type: decision_for
- target: comp-hermes-fts5-schema
  type: decision_for
---
**Adopt three Hermes subsystems only: skill evolution pipeline, FTS5 schema, Docker backend** (§17, §2.9).

What we explicitly do NOT take: Hermes' top-level Maestro loop, tool registry, messaging gateway, scheduler, skill memory ("dialectical user model"), messaging-platform integrations.

Why: Hermes is best-in-class at three specific things; we adopt those as subsystems. Adopting Hermes wholesale would impose its worldview on our top-level architecture (`principle-adopt-subsystems-not-frameworks`).