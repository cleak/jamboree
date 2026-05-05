---
scope: harnesses/claude-code
---

# Claude Code — Harness Routing Affinity

Anthropic's first-party agentic coding harness. Auth via Claude Pro/Max subscription.

<capabilities>
- `supports_interrupt`: true (Esc key cancels mid-stream).
- `supports_message_queue`: true.
- `supports_worktree_isolation`: true.
- `supports_thinking_mode`: yes (extended thinking for architectural work).
- `supports_session_resume`: limited (per-Claude-Code-instance).
- `supports_session_start_hook`: **yes** — `.claude/settings.json` `SessionStart`/`SessionEnd` hooks. Used for `tempyr journal bootstrap`/`finalize`.
- `auth_modes`: `Subscription` (Pro/Max), `ApiKey` (fallback).
- `default_sandbox_backend`: `local`. Migration to `docker` is supported.
</capabilities>

<quota_mechanics>
Rate-limit shape per Anthropic's published docs. Per-tier:
- Pro: 1x baseline.
- Max5x: 5x.
- Max20x: 20x.

Per-day session count + per-session message count. Claude Code Pickers count against the same quota as the Manager's Claude Code use; the quota tracker keeps both visible.

The April 4 2026 Anthropic block on third-party harnesses does **NOT** apply to Claude Code itself — first-party.
</quota_mechanics>

<tempyr_journal_integration>
Claude Code reads `.claude/settings.json` from the Picker's worktree. The harness adapter writes:
```json
{
  "hooks": {
    "SessionStart": [{"hooks": [{"command": "tempyr journal bootstrap --quiet", "type": "command"}]}],
    "SessionEnd": [{"hooks": [{"command": "tempyr journal finalize --agent claude --quiet", "type": "command"}]}]
  }
}
```

Bootstrap is non-fatal; finalize is best-effort. If a Claude Code Picker is `full-stop`'d before SessionEnd fires, the harness adapter's cleanup path runs `finalize` from the supervisor side.

Claude Code also auto-discovers Blueberry's `.claude/skills/` (procedural-animation, sdf-modeling, tempyr-interview, tempyr-ops) and `.claude/agents/` (anim-reviewer, sdf-reviewer, tempyr-extractor). See `projects/blueberry/blueberry-skill-pack-bridge.md`.
</tempyr_journal_integration>

<when_to_dispatch_claude_code>
**Good fit:**
- Architectural / cross-system refactors.
- Deep code review (the `code-review` skill pack family).
- SDF art asset work (Blueberry has `sdf-modeling` skill pack tuned for Claude Code).
- Animation-system work (Blueberry has `procedural-animation` skill pack).
- Tasks where extended thinking helps (mathematical correctness, invariant analysis).
- Long-context work (~200K input tokens; Claude handles context length well).
- Task-types: `risky-architecture`, `ecs-refactor` (substantial refactors).

**Less good fit:**
- Tight feedback loops with many small tool calls (Codex CLI is faster here).
- Overnight batch where DeepSeek's pricing wins.
- Tasks where the model's tool-call discipline matters more than reasoning depth.
</when_to_dispatch_claude_code>

<operational_notes>
- Subscription tokens land in `~maestro/.claude/...` (mode 700, owned by maestro). First launch via `claude` triggers OAuth flow; the harness adapter does this once at install time.
- Per `dec-chatgpt-subscription-oauth-for-maestro`, the Maestro itself uses Codex OAuth, not Claude Code. Claude Code is only for spawned Pickers.
- `.claude/settings.json` lives in the Picker's worktree. The harness adapter writes the hooks config before spawn; Blueberry's main checkout `.claude/settings.json` is irrelevant to Picker-side configuration.
</operational_notes>

<extended_thinking>
Claude Code supports extended thinking modes. For tasks where mathematical correctness or architectural analysis matters, set the prompt to invoke thinking. The harness adapter doesn't need to configure this explicitly — Claude Code adapts based on prompt cues.

Don't enable extended thinking for routine work — it inflates token cost without commensurate benefit.
</extended_thinking>

<dispatch_template>
```
spawn-picker(spec={
    task_id: "...",
    harness: "claude-code",
    sandbox_backend: "local" | "docker",
    sandbox_profile: "default" | "hardened",
    task_class: "ecs-refactor" | "risky-architecture" | "compile-heavy-rust" | ...,
    initial_prompt: """
        <task description>

        Context:
        - Project: Blueberry (Bevy 0.18 voxel game).
        - Spec: {relevant spec sections}.
        - Acceptance: {explicit criteria}.

        Apply Blueberry's commit and PR validation gates.
    """,
    budget_usd: 5.00 - 15.00 (depending on task class),
})
```
</dispatch_template>

<related>
- `harnesses/codex-cli.md` — when to route there instead.
- `harnesses/opencode-deepseek.md` — when API-tier wins.
- `projects/blueberry/blueberry-skill-pack-bridge.md` — Blueberry's `.claude/skills/` integration.
- `task-types/ecs-refactor.md`, `task-types/risky-architecture.md` — Claude Code's sweet spots.
</related>
