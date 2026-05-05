---
id: risk-sqlite-vs-postgres
type: risk
status: accepted
created: 2026-05-04T03:47:05.764569639Z
updated: 2026-05-04T03:47:05.764570166Z
---
**§13.10 SQLite vs Postgres.** SQLite scales fine for one-developer workloads but breaks at multi-machine or high concurrency.

Mitigation: schema and queries written portably; migration to Postgres if needed is straightforward. Not a real concern for solo use.