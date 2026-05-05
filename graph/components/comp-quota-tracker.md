---
id: comp-quota-tracker
type: component
status: planned
created: 2026-05-04T03:31:39.127899394Z
updated: 2026-05-04T04:40:55.741008096Z
edges:
- target: comp-jam-svc-observe
  type: depended_on_by
- target: feat-quota-tracking
  type: used_by
- target: feat-substrate-services
  type: used_by
---
Tracks all three quota shapes uniformly (§4.4.5). Owned by `jam-svc-observe` and exposed via `world-snapshot.harness_quotas`. Token counting per harness via process-side instrumentation (parsing harness logs/response metadata).

`PriceEvent` exposes things like "DeepSeek's 75% sale ends 2026-05-31 15:59 UTC" so the Maestro can plan around upcoming cost changes. Config-loaded; we don't try to detect price changes automatically.

Conservative-by-default (under-estimate remaining quota); periodic re-sync via observed limit responses; manual re-sync via `jam quota recalibrate` (§13.3).

Subscription windows tracked from observed limit-hit events plus published reset cadences.