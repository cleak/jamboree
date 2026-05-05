---
id: risk-search-backend-deprecation
type: risk
status: identified
created: 2026-05-04T03:47:10.920677947Z
updated: 2026-05-04T03:47:10.920678463Z
---
**§13.13 Search backend deprecation.** A search API we depend on could be sunset, acquired, or pricing-shifted.

Mitigation: router with multiple backends and cooldown; failover automatic; new backends are config additions.