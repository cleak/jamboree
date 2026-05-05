---
id: principle-subscription-friendly-api-when-necessary
type: constraint
status: active
created: 2026-05-04T03:23:48.815508244Z
updated: 2026-05-04T04:29:43.185135292Z
edges:
- target: comp-claude-code-adapter
  type: constrains
- target: comp-codex-cli-adapter
  type: constrains
- target: comp-opencode-deepseek-adapter
  type: constrains
- target: feat-picker-layer-three-tier
  type: constrains
- target: feat-quota-tracking
  type: constrains
---
**§2.10 Subscription-friendly where possible, API where necessary.**

For a single-developer overnight orchestrator, subscriptions amortize better than API billing. ChatGPT Pro $100/$200 covers Codex CLI within rolling 5-hour windows; Claude Pro/Max covers Claude Code; DeepSeek's API is cheap enough that overflow workloads cost less than subscription floors.

Architectural implication: the quota tracker understands both subscription windows (rolling, tier-multiplied, harness-specific) and API budgets (monthly cap, per-token rates, sale expiry). Subscription-tier work in normal operation; API-tier (OpenCode+DeepSeek) for burst, low-stakes high-volume work, or when subscriptions are exhausted.

*Why:* a system that runs at subscription cost levels but bursts to API on demand is structurally cheaper than all-API, while staying flexible enough for scale that pure-subscription can't handle.