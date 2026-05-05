---
scope: harnesses/codex-cli
---

# Codex CLI — Harness Routing Affinity

OpenAI's first-party agentic coding harness. Auth via ChatGPT subscription (Pro $100 = 5x Plus, Pro $200 = 20x Plus). Built-in worktree support, parallel project execution, Skills integration, Automations.

## Capabilities

- `supports_interrupt`: true (clean cancellation between tool calls).
- `supports_message_queue`: true.
- `supports_worktree_isolation`: true.
- `supports_thinking_mode`: yes (configurable).
- `supports_session_resume`: yes (Codex's internal session mechanism).
- `supports_session_start_hook`: **yes** — clean SessionStart/SessionEnd integration with `tempyr journal bootstrap` / `finalize`.
- `auth_modes`: `Subscription` (primary), `ApiKey` (fallback).
- `default_sandbox_backend`: `local`. Migration to `docker` is supported.

## Quota mechanics

5-hour rolling windows on three message types:
- **`local-messages`** — interactive work in the terminal.
- **`cloud-tasks`** — delegated background work.
- **`code-reviews`** — PR review-driven sessions.

Tier multipliers:
- Plus: 1x.
- Pro $100: 5x.
- Pro $200: 20x.

**Speed-mode** burns credits faster — disabled by default. Enable per-task via skill flag when latency matters more than rate-limit headroom.

## Tempyr journal integration

Codex CLI supports `SessionStart` / `SessionEnd` hooks via `.codex/environments/environment.toml`. The orchestrator's harness adapter writes hook config that:
- On SessionStart: `tempyr journal bootstrap --quiet`.
- On SessionEnd: `tempyr journal finalize --agent codex --quiet`.

This means Codex Pickers automatically open and close their Tempyr session correctly without per-Picker bookkeeping.

If a Codex Picker is `full-stop`'d before SessionEnd fires, the harness adapter's cleanup path runs `finalize` from the supervisor side.

## When to dispatch Codex CLI

**Good fit:**
- Routine coding tasks where the Manager will be available to interject.
- Tasks where the model's strengths in tool-call sequencing matter (Codex is excellent here).
- Tasks where Skills + AGENTS.md project config integrate cleanly with Blueberry.
- Compile-heavy-rust tasks during business hours when 5h windows align.

**Less good fit:**
- Long architectural refactors where Claude Code's deep reasoning is preferred (route to `claude-code` instead).
- Overnight batch jobs where DeepSeek's pricing wins (route to `opencode-deepseek` for compile-heavy or low-stakes).
- Tasks needing > 200K input tokens — Codex's context limit is real.

## Operational notes

- **Subscription tokens land in `~maestro/.codex/auth.json`** (mode 600, owned by maestro). Per `dec-chatgpt-subscription-oauth-for-maestro`, Codex OAuth via `codex login --device-auth` is the auth mechanism.
- The same Codex OAuth credential powers Codex-based Pickers — no separate harness token needed.
- AGENTS.md project config in Blueberry pattern-matches Codex CLI's expectations. Skills transfer cleanly.

## Quota observability

The quota tracker (`comp-quota-tracker`) watches Codex's response metadata for limit-hit signals. Conservative-by-default — under-estimate remaining quota. When the 5h window resets, the tracker observes the reset event and updates.

If `query-quota(harness-id="codex-cli")` returns `local-messages` with `used_in_window` near `limit_in_window`, prefer routing the next task to Claude Code or OpenCode rather than burning the last few messages.

## Speed-mode usage

Speed-mode burns credits ~2x faster but reduces latency. Per-task skill flag:
```yaml
codex-cli:
  speed-mode: true
```

Enable when:
- The Manager is waiting on the result.
- The task is tightly bounded (< 10min wall-clock).
- Quota headroom is comfortable.

Don't enable for overnight batch — speed-mode wastes quota for no human benefit.

## Worktree behavior

Codex CLI handles worktrees natively but Jamboree's `comp-worktree-create-protocol` (§6.9) creates the worktree before Codex starts. Codex inherits `cwd` and operates within it.

Worktree path under multi-user model: `/home/picker/workers/<task-id>/`. Codex sees this as its cwd; doesn't need to know about the orchestrator side.

## Known limitations

- Codex's internal session resume is per-Codex-instance, not cross-restart. If the Picker process dies, the session state is gone — Jamboree's spawn-create-fresh model handles this.
- Codex's tool-call format is OpenAI-shaped. The harness adapter normalizes responses into the orchestrator's typed events.

## Related skills

- `harnesses/claude-code.md` — when to route there instead.
- `harnesses/opencode-deepseek.md` — when API-tier is the better choice.
- `task-types/compile-heavy-rust.md` — Codex's sweet spot for Bevy work.
