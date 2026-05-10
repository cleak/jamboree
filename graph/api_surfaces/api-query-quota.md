---
id: api-query-quota
type: api_surface
status: stable
created: 2026-05-04T03:51:57.497626658Z
updated: 2026-05-06T21:18:32Z
edges:
- target: comp-jam-svc-observe
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`query-quota(harness-id?)` → `HarnessQuotaState` or full quota map (§5.1, §4.4.5).

Three quota shapes (Codex 5h windows / Claude tier / API budget). `PriceEvent` exposes scheduled price changes.

Implementation note (2026-05-06): `tool.observe.query-quota` accepts `{harness_id?}`. Without a harness it returns the full `harness/window_kind` map; with a harness it returns matching entries or `quota-not-found`. The backing data is journal-derived from `quota.exhausted`, `quota.exhausted-soon`, `quota.refilled`, and `quota.usage-observed`, augmented by optional project-config metadata for `reset_cadence`, `api_budget`, and `price_events`. `jam quota show` is now a CLI consumer of this API and preserves the same filter semantics.
