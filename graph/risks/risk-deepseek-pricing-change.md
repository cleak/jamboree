---
id: risk-deepseek-pricing-change
type: risk
status: identified
created: 2026-05-04T03:47:07.489178783Z
updated: 2026-05-04T03:47:07.489179578Z
---
**§13.11 DeepSeek pricing change after 2026-05-31.** The 75% sale ends. Regular pricing is still cost-effective but ~3-7x more expensive at the API tier.

Mitigation: skill files note the date; Maestro monitors price events (via `PriceEvent` in `ApiBudgetState`); orchestrator can shift more work to subscription harnesses if API costs balloon.