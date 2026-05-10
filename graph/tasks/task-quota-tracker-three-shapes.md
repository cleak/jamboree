---
id: task-quota-tracker-three-shapes
type: task
status: blocked
created: 2026-05-04T03:59:13.425223891Z
updated: 2026-05-06T19:20:10Z
edges:
- target: feat-quota-tracking
  type: child_of
---
Phase 3 (§12). Quota tracker for all three harness shapes (Codex CLI 5h windows, Claude rate-limit, OpenCode/DeepSeek API budget).

Per `comp-quota-tracker`, `feat-quota-tracking`, `risk-quota-tracker-accuracy`.

Acceptance: burn Codex CLI quota by hand and watch the Maestro route subsequent tasks elsewhere.

Implementation note (2026-05-06): landed the first journal-derived quota tracker slice in `jam-svc-observe`. `journal.quota.jsonl` entries for `quota.exhausted`, `quota.exhausted-soon`, and `quota.refilled` become `world-snapshot.harness_quotas` states keyed by `harness/window_kind`; `tool.observe.query-quota` can return the full map or filter by harness. Live smoke used temporary NATS and a temporary quota journal: `tool.observe.query-quota {"harness_id":"codex-cli"}` returned `codex-cli/local-messages` as exhausted, and `tool.observe.world-snapshot` included both `codex-cli/local-messages` exhausted and `opencode-deepseek/api-budget` available with quota freshness `fresh`.

CLI note (2026-05-06): `jam quota show` now opens a traced request to `tool.observe.query-quota`, prints a stable tabular view, and supports `--harness-id` filtering plus `--nats-url` / `--timeout-secs`. `jam quota recalibrate` now manually publishes the existing quota journal events for `available` (`quota.refilled`), `exhausted` (`quota.exhausted`), and `low` (`quota.exhausted-soon`) states. Live smoke with temporary NATS + `jam-nats-bridge` + `jam-svc-observe` published `journal.quota.exhausted` and `journal.quota.refilled`, verified they landed in `journal.quota.jsonl`, and confirmed `jam quota show --harness-id codex-cli` reflected each correction.

Config/instrumentation note (2026-05-06): `jam-svc-observe` now merges optional quota metadata from `JAM_QUOTA_CONFIG`, `JAM_PROJECT_CONFIG`, or an existing `~/.jam/config/projects/blueberry.toml`. It exposes `reset_cadence`, `api_budget`, `usage`, and `price_events` on the same `harness/window_kind` quota states returned by `world-snapshot.harness_quotas` and `tool.observe.query-quota`; `jam quota show` prints those fields when present. OpenCode runner output is teed to `.jam/opencode-events.jsonl`; direct Codex and Claude launches truncate stdout into `.jam/codex-events.jsonl` and `.jam/claude-events.jsonl`. `jam-svc-session` parses common JSON usage fields after process exit and publishes `journal.quota.usage-observed` without exposing NATS credentials to the Picker. Focused tests cover config-merged reset cadence, API budget remaining fraction, price-event serialization, and per-harness usage JSON routing.

Usage smoke (2026-05-06): `/tmp/jam-quota-usage-smoke-Z4Ypju` ran temporary NATS, `jam-nats-bridge`, `jam-svc-session`, and `jam-svc-observe` with a fake pinned OpenCode binary and fake worktree responder. `tool.session.spawn-picker` produced `opencode-deepseek:01KQYNE98HCH3G2NDDSV9SJFHK`; the bridge wrote `journal.quota.jsonl` with `quota.usage-observed` payload `{input_tokens:1234, output_tokens:321, cost_usd:0.42, source:"opencode-json"}`; `tool.observe.query-quota {"harness_id":"opencode-deepseek"}` returned `usage` with those values and folded spend into the configured API budget (`spent_this_month_usd=1.42`, `remaining=0.858`). A second temporary NATS smoke with a fake pinned Codex binary produced `codex-cli:01KQYP8ZTV148YZQRB4YN33BSC`, captured stdout into `.jam/codex-events.jsonl`, wrote `quota.usage-observed`, and `tool.observe.query-quota {"harness_id":"codex-cli"}` returned `codex-cli/local-messages` usage `{input_tokens:42, output_tokens:17, cost_usd:0.003, source:"codex-json"}`. Real one-word schema samples verified Codex emits usage on `type=turn.completed` and Claude emits both assistant usage plus a final `type=result` summary; the parser now prefers the Claude result summary and reads `total_cost_usd` / `modelUsage` aliases to avoid double counting. Remaining work: service-level real harness smoke under the production launch path and the acceptance test that burns real Codex CLI quota and verifies dispatch rerouting.

Blocked note (2026-05-06): the remaining acceptance is deliberately real-world:
burn real Codex CLI quota and watch dispatch reroute. That cannot be completed
as a safe unattended local smoke because it consumes subscription quota and
needs a live multi-harness runtime. The tracker implementation and synthetic
smokes are complete; local runtime now has `jam/pickers/deepseek-api-key`, so
unblock by scheduling a manual quota-burn window with the runtime services up
and a real OpenCode reroute target available.
