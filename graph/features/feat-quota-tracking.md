---
id: feat-quota-tracking
type: feature
status: active
created: 2026-05-04T03:28:24.775673094Z
updated: 2026-05-06T13:21:57Z
owner: caleb
edges:
- target: comp-jam-svc-observe
  type: uses
- target: comp-quota-tracker
  type: uses
- target: jamboree-v5
  type: child_of
- target: principle-observable-not-deterministic
  type: constrained_by
- target: principle-provider-agnostic-everywhere
  type: constrained_by
- target: principle-subscription-friendly-api-when-necessary
  type: constrained_by
- target: task-dispatch-policy-quota-skill-driven
  type: parent_of
- target: task-quota-tracker-three-shapes
  type: parent_of
---
Three quota shapes tracked uniformly (§4.4.5). Exposed to Maestro via `world-snapshot.harness_quotas`.

```rust
pub enum HarnessQuotaState {
    Codex(CodexQuota),         // 5h rolling, tier multipliers, message types
    ClaudeCode(ClaudeQuota),   // Pro/Max rate limit shape
    OpenCode(ApiBudgetState),  // dollar burn, per-model rate limits
    Specialized(HashMap<String, BudgetState>),
}
```

Codex window types: `local-messages`, `cloud-tasks`, `code-reviews`. Speed-mode credits.
Claude window: per-tier rate limit + `session_count_today`.
API budget: `monthly_cap_usd`, `spent_this_month_usd`, `current_input_rate_per_1m`, rate-limit state, `PriceEvent` (e.g. DeepSeek 75% sale ending 2026-05-31).

Token counting via process-side instrumentation (parsing harness logs/response metadata) rather than guessing. Subscription windows tracked from observed limit-hit events plus published reset cadences.

Conservative-by-default (under-estimate remaining quota); periodic re-sync via observed limit responses; manual re-sync via `jam quota recalibrate` (§13.3).

Implementation note (2026-05-06): `jam-svc-observe` now exposes a first-pass quota view from journaled `quota.*` events plus optional project-config metadata. It covers observed exhausted, low, refilled/available, and usage-observed states for Codex-style windows, Claude rate-limit windows, and API-budget windows by using the event `harness` and `window_kind` fields; config can add `reset_cadence`, `api_budget`, and `price_events` to the same states. `jam quota show` uses the same traced `tool.observe.query-quota` surface for Manager-facing inspection, and `jam quota recalibrate` can manually publish the same state correction event shapes for resync. OpenCode and fake Codex process-side usage paths have NATS smokes from JSON event logs; real Codex/Claude one-word schema samples now back the parser aliases and Claude result-summary preference.

Dispatch note (2026-05-06): the Maestro session loop now consumes `world-snapshot.harness_quotas` for a first dispatch plan. Exhausted candidate harnesses are skipped, low quota is de-prioritized, and task-type skills provide the initial harness ordering. Runtime loops now call routed `session.spawn-picker` for chosen harnesses and record Picker handles or typed spawn errors; a temporary NATS smoke proved this path for fake pinned Codex. Claude Code live spawning has landed; the dispatch-policy acceptance remains blocked on the OpenCode session adapter's real DeepSeek key and the final three-harness parallel-spawn proof.
