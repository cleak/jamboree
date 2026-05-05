---
id: risk-picker-burning-money
type: risk
status: identified
created: 2026-05-04T03:47:27.875386294Z
updated: 2026-05-04T03:47:27.875386911Z
---
**Threat #4 (low likelihood for Caleb's setup, security-setup §1).** Picker calling expensive APIs in a loop.

Bounded by quota tracker for instrumented harnesses; budget-cap escapes from this protection are possible if the Picker calls non-orchestrated APIs.

Multi-user model is orthogonal to this risk (handled by quota tracking and per-session budgets in v5).