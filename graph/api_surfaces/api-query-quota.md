---
id: api-query-quota
type: api_surface
status: draft
created: 2026-05-04T03:51:57.497626658Z
updated: 2026-05-04T04:53:44.333843408Z
edges:
- target: comp-jam-svc-observe
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`query-quota(harness-id?)` → `HarnessQuotaState` or full quota map (§5.1, §4.4.5).

Three quota shapes (Codex 5h windows / Claude tier / API budget). `PriceEvent` exposes scheduled price changes.