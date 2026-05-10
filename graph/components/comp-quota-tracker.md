---
id: comp-quota-tracker
type: component
status: active
created: 2026-05-04T03:31:39.127899394Z
updated: 2026-05-06T13:09:36Z
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

Implementation note (2026-05-06): the first observable quota state is active inside `jam-svc-observe`. It reads `journal.quota.jsonl` entries for `quota.exhausted`, `quota.exhausted-soon`, `quota.refilled`, and `quota.usage-observed`, keys states by `harness/window_kind`, then merges optional project-config metadata for reset cadence, API budget shape, and price events. The merged map is exposed in `world-snapshot.harness_quotas` plus `tool.observe.query-quota`. `jam quota show` now calls that tool surface for CLI inspection, and `jam quota recalibrate` manually publishes the state correction event shapes. OpenCode and fake Codex process-side JSON usage parsing/publication have live NATS smokes; real Codex/Claude one-word schema samples now back the parser aliases and Claude de-duplication behavior, with dispatch rerouting still pending.
