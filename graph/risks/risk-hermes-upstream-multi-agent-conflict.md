---
id: risk-hermes-upstream-multi-agent-conflict
type: risk
status: identified
created: 2026-05-04T03:47:02.356842528Z
updated: 2026-05-04T03:47:02.356843198Z
---
**§13.8 Hermes upstream multi-agent work conflict.** If Hermes' upstream develops conflicting multi-agent abstractions, our subsystem-only adoption could become stale.

Mitigation: we use specific stable subsystems (FTS5 schema, DSPy+GEPA pipeline shape, Docker backend); if Hermes pivots, our code keeps working since we vendored.