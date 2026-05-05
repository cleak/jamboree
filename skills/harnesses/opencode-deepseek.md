---
scope: harnesses/opencode-deepseek
---

# OpenCode + DeepSeek V4 Pro — Harness Routing Affinity

OpenCode is the open-source terminal-native harness, configured with DeepSeek V4 Pro as default model. Pay-per-use API.

<capabilities>
- `supports_interrupt`: yes (harness-specific protocol).
- `supports_message_queue`: yes.
- `supports_worktree_isolation`: yes.
- `supports_thinking_mode`: yes (DeepSeek's reasoning mode).
- `supports_session_resume`: limited.
- `supports_session_start_hook`: **NO** native hooks. The harness adapter wraps OpenCode invocation: prefix with `tempyr journal bootstrap`, append `tempyr journal finalize` to cleanup. If a Picker is `full-stop`'d before the wrapper runs cleanup, supervisor-side cleanup runs `finalize`.
- `auth_modes`: `ApiKey` (DeepSeek API key required).
- `default_sandbox_backend`: `local`. Docker is supported.
</capabilities>

<pricing>
**Sale pricing (until 2026-05-31 15:59 UTC):** $0.435 / $0.87 per 1M tokens (input/output).
- 11–34x cheaper than GPT-5.5 at the API tier.
- Captured as `metric-deepseek-pricing-events`.

**Regular pricing (post-sale):** $1.74 / $3.48 per 1M tokens.
- Still 3-7x cheaper than the subscription harnesses' API-equivalent rates.
- Captured as `risk-deepseek-pricing-change`.

**V4 Flash** (`$0.14 / $0.28` per 1M): cheaper variant, used for low-stakes background work. Maestro can specify `model_override: "deepseek-v4-flash"` per task within this harness.
</pricing>

<latency_caveat>
**DeepSeek V4 Pro at max reasoning effort runs ~33 tokens/sec.** Verbose by Artificial Analysis measurement.

Implication:
- **NOT a fit for latency-sensitive interactive work.**
- **NOT a fit for tight feedback loops with many small tool calls.**
- **IS a fit for overnight batch jobs** where wall-clock latency is acceptable.
- **IS a fit for compile-heavy refactors** where the cargo build dominates wall-clock anyway.

A typical 30-minute coding session at sale pricing: $0.10 - $0.50.
</latency_caveat>

<benchmarks>
- 80.6% SWE-bench Verified.
- 93.5 LiveCodeBench (highest among publicly accessible models as of Q1 2026).
- 67.9% Terminal-Bench 2.0.
- 1M-token context window (genuinely useful for whole-codebase analysis).
</benchmarks>

<tempyr_journal_integration>
No native `SessionStart`/`SessionEnd` hooks. Harness adapter wraps:
```bash
# pseudocode
tempyr journal bootstrap --quiet --worktree $WORKTREE
opencode --config ... <prompt>
tempyr journal finalize --agent opencode --worktree $WORKTREE --quiet
```

If full-stop fires mid-task, the wrapper's cleanup phase doesn't run. Supervisor's `comp-jam-svc-supervise` invokes `tempyr journal finalize` from outside as a fallback.

Pickers should still log `plan` / `finding` / `decision` / `outcome` entries via `tempyr journal log` calls during the session — the bootstrap/finalize wrapper opens/closes the session, but the agent fills it.
</tempyr_journal_integration>

<when_to_dispatch_opencode>
**Good fit:**
- Overnight batch jobs (cost matters; latency doesn't).
- Compile-heavy Rust where compile time dominates anyway.
- Whole-codebase analysis tasks where 1M context is genuinely useful.
- Low-stakes high-volume work (doc generation, comment cleanup, mechanical refactors).
- Burst capacity when subscription harnesses are exhausted (Codex CLI 5h windows full, Claude Pro/Max rate limit hit).
- Task-types: `compile-heavy-rust`, `doc-generation`, `light-edit` (when Codex/Claude unavailable).

**Less good fit:**
- Manager-waiting-for-result work (latency too high).
- Tight tool-loop tasks (DeepSeek's reasoning slows things down).
- Tasks where the Manager wants extended thinking quality (Claude Code's better here).
- Tasks requiring `procedural-animation` or `sdf-modeling` Blueberry skill packs (those are tuned for Claude Code's discovery).
</when_to_dispatch_opencode>

<operational_notes>
- API key in pass at `jam/pickers/deepseek-api-key`.
- Endpoint options: OpenAI-compatible (`https://api.deepseek.com`) or Anthropic-compatible (`https://api.deepseek.com/anthropic`). The harness adapter chooses based on OpenCode's config.
- AGENTS.md project config in Blueberry pattern-matches OpenCode's expectations. Skill files transfer cleanly between Codex CLI and OpenCode harnesses.
- Models.dev provides 75+ providers via OpenCode if we ever need to swap underlying models — DeepSeek is the default, not a hard requirement.
</operational_notes>

<budget_routing>
Per `principle-subscription-friendly-api-when-necessary`:
- Subscription harnesses (Codex/Claude Code) for routine work in normal operation.
- OpenCode + DeepSeek for overflow / overnight / cost-sensitive batch.

Quota-tracker integration: `world-snapshot.harness_quotas` includes OpenCode's `ApiBudgetState` with `monthly_cap_usd`, `spent_this_month_usd`, current rates, and `PriceEvent` entries (e.g. "DeepSeek 75% sale ends 2026-05-31").

When `ApiBudgetState.spent_this_month_usd / monthly_cap_usd > 0.8`, the Maestro should prefer subscription harnesses if they have headroom, only falling back to OpenCode for genuinely API-required work.
</budget_routing>

<dispatch_template>
```
spawn-picker(spec={
    task_id: "...",
    harness: "opencode",
    sandbox_backend: "local" | "docker",
    sandbox_profile: "default",
    task_class: "compile-heavy-rust" | "doc-generation" | "light-edit",
    initial_prompt: """
        <task description>

        Context:
        - Project: Blueberry (Bevy 0.18).
        - Spec / refs: {...}.
        - Acceptance: {explicit criteria}.

        Note: this is an overnight / cost-sensitive run — prioritize correctness over speed.
        Apply Blueberry's commit and PR validation gates.
    """,
    model_override: "deepseek-v4-pro" | "deepseek-v4-flash",
    budget_usd: 1.00 - 5.00,
})
```
</dispatch_template>

<related>
- `harnesses/codex-cli.md` — subscription tier alternative.
- `harnesses/claude-code.md` — when extended thinking is preferred.
- `task-types/compile-heavy-rust.md` — OpenCode's sweet spot.
- `risk-deepseek-pricing-change` — sale ends 2026-05-31; revisit pricing then.
</related>
