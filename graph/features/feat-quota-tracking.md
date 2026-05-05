---
id: feat-quota-tracking
type: feature
status: draft
created: 2026-05-04T03:28:24.775673094Z
updated: 2026-05-04T04:24:22.109851632Z
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