---
id: risk-world-snapshot-freshness
type: risk
status: identified
created: 2026-05-04T03:46:55.620486622Z
updated: 2026-05-04T03:46:55.620487137Z
---
**§13.4 World-snapshot freshness.** Event-driven invalidation reduces but doesn't eliminate the staleness window. If GitHub webhooks lag or PR poller misses an event, snapshot can be briefly stale.

Mitigation: 60s TTL backstop; Maestro can request `refresh-world-snapshot` when it suspects staleness.