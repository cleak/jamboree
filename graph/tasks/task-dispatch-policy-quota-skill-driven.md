---
id: task-dispatch-policy-quota-skill-driven
type: task
status: blocked
created: 2026-05-04T03:59:16.231116684Z
updated: 2026-05-06T20:37:56Z
edges:
- target: feat-quota-tracking
  type: child_of
---
Phase 3 (§12). Dispatch logic: Maestro uses quota and skill files to pick a harness per task.

Per `feat-quota-tracking`, `principle-subscription-friendly-api-when-necessary`, `dec-three-tier-picker-pool`.

Acceptance: spawn 3 Pickers across 3 different harnesses in parallel; each runs in its own worktree, journals to Tempyr correctly, completes or fails cleanly.

Implementation note (2026-05-06): added the first Maestro-side dispatch policy in `maestro/src/jam_maestro/dispatch.py` and integrated it into the session-loop decision shape. The policy reads task-type skill spawn templates for harness preference, canonicalizes `opencode` to `opencode-deepseek`, ranks candidates by `world-snapshot.harness_quotas`, de-prioritizes low quota, and blocks when all candidate quotas are exhausted. A live smoke with temporary NATS + `jam-svc-observe` loaded real skills and a real quota-backed world snapshot; with `codex-cli/local-messages` exhausted and `opencode-deepseek/api-budget` available, a `light-edit` wake produced `dispatch-ready:opencode-deepseek` and a planned `SessionSpawnPickerRequest`. The runtime Maestro loop now wires a routed NATS session client, calls `tool.session.spawn-picker` when dispatch chooses a harness, records the returned Picker handle, and converts session-tool errors into `blocked:spawn-picker-error`. Remaining acceptance work: run a real OpenCode/DeepSeek V4 Pro spawn, then prove three-harness parallel spawns and Tempyr journal closure end to end.

Maestro spawn smoke (2026-05-06): temporary NATS + `jam-svc-observe` + `jam-svc-session` + fake pinned Codex proved the runtime loop can go from `world-snapshot` to quota-aware dispatch to a real traced `tool.session.spawn-picker` call. The returned session decision was `spawned:codex-cli`, included the `PickerHandle`, and the fake harness wrote `.jam/codex-events.jsonl`.

Session-service note (2026-05-06): `jam-svc-session` now supports live `codex-cli`, `claude-code`, and `opencode-deepseek` spawns. The OpenCode adapter path has fake-harness NATS smoke coverage for normal exit and `full-stop` Tempyr closure; live three-harness acceptance remains blocked on paid provider/runtime verification rather than local DeepSeek credential discovery.

Credential note (2026-05-06): local runtime now has the canonical
`jam/pickers/deepseek-api-key` in maestro pass, so dispatch is no longer blocked
on local DeepSeek credential discovery.

Blocked note (2026-05-06): the three-harness parallel acceptance still needs a
paid OpenCode path verification and live Codex, Claude, and OpenCode spawns in
parallel. Run this during an approved quota window and verify each worktree plus
Tempyr journal closes cleanly.

Budget guard note (2026-05-06): the Maestro dispatch policy now treats
API-budget remaining dollars as part of the quota disposition. It compares the
remaining `api_budget.monthly_cap_usd - spent_this_month_usd` from
`world-snapshot.harness_quotas` against the task-class `budget_usd`; a paid
harness is exhausted for that task when remaining spend is below the task
budget, and low when remaining spend is less than 2x the task budget. This keeps
the OpenCode/DeepSeek overflow path conservative before a paid spawn is made.
Coverage: `uv run --directory maestro pytest`, `ruff check .`, and `pyright`.
