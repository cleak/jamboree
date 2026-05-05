---
id: risk-hermes-evolution-maintenance-burden
type: risk
status: identified
created: 2026-05-04T03:47:00.637513132Z
updated: 2026-05-04T03:47:00.637513813Z
---
**§13.7 Hermes evolution pipeline maintenance burden.** The DSPy + GEPA pipeline depends on Python ML libraries that evolve fast and have heavy installs.

Mitigation: vendored as a Python virtualenv with pinned versions; updates batched manually rather than auto-applied.