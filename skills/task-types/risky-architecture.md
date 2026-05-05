---
scope: task-types/risky-architecture
---

# Task Type — Risky Architecture

Tasks that touch foundational architecture in ways that could go wrong silently — non-trivial SDF art assets, cross-cutting refactors, anything where a mistake is hard to detect by tests alone.

<concurrency_cap>
**1 concurrent globally** (per spec §6.7 for Blueberry). One risky-architecture task at a time. The cap is a hard substrate-enforced limit.
</concurrency_cap>

<sandbox_profile>
**`hardened × docker`** by default. Why:
- Blast-radius containment matters more here than for routine compile-heavy work.
- Network is restricted to harness API + GitHub + crates.io + project allowlist.
- Read-only main repo + read-write worktree mount.

Use `hardened × modal` for elastic burst when local Docker resources are saturated.
</sandbox_profile>

<harness_selection>
**Claude Code, with extended thinking.** Reasoning depth matters more than tool-call speed.

Codex CLI works for narrower risky tasks (one crate, one foundational change) where the architecture isn't being re-thought.

Avoid OpenCode + DeepSeek for risky-architecture work unless the Manager explicitly approves — DeepSeek's verbosity and slower tool-loops compound risk.
</harness_selection>

<what_qualifies>
Examples of risky-architecture tasks for Blueberry:
- New SDF art asset systems (use the `sdf-modeling` Blueberry skill pack).
- Cross-cutting changes to `ObjectIdPlugin` / shadow mesh sync.
- Major changes to `src/display.rs` (display scaling, window management).
- New rendering passes or post-process changes.
- New ECS plugin architecture.
- Changes to the `agent_tools/` BRP server (could affect all Pickers).
- Multi-crate refactors with non-trivial shared invariants.

Examples that are NOT risky-architecture:
- A single function refactor.
- Test additions.
- Documentation changes.
- CodeRabbit-driven small refactors.
- Bug fixes with isolated scope.
</what_qualifies>

<budget>
Higher than routine compile-heavy:
- Single foundational change: $15 - $40.
- Cross-cutting risky refactor: $40 - $100.

Risky tasks consume more reasoning tokens. Don't underbudget — risk of hard-abort mid-task.
</budget>

<spawn_template>
```
spawn-picker(spec={
    task_id: "...",
    harness: "claude-code",
    sandbox_backend: "docker",
    sandbox_profile: "hardened",
    task_class: "risky-architecture",
    initial_prompt: """
        Risky architecture task: <description>

        Why this is risky-architecture:
        - <specific reason: cross-cutting, foundational, silent-failure-prone>.

        Apply Blueberry's invariants (skills/projects/blueberry/code-conventions.md):
        - Compile errors over runtime errors.
        - Loose coupling via events.
        - State machines with explicit terminal phases.
        - Don't bypass display.rs / ObjectIdPlugin / collision-layers conventions.

        Specific safeguards for this task:
        - <task-specific invariants>.

        Use plan mode (changes span 3+ files almost certainly).
        For SDF art: invoke the sdf-modeling skill (Blueberry's .claude/skills/).
        For animation: invoke procedural-animation skill.

        Acceptance:
        - cargo fmt --check + cargo clippy --workspace --all-targets -- -D warnings.
        - cargo test --workspace.
        - Regression tests added.
        - For visual changes: BRP smoke test passes (see skills/projects/blueberry/brp-server.md).
        - For perf-relevant changes: cargo bench before/after, no >1% regression.
        - PR opened ready-for-review with detailed rationale in the body.
    """,
    budget_usd: 15.00 - 100.00,
})
```
</spawn_template>

<escalation_rules>
Escalate via `notify-human(urgency=medium)` BEFORE dispatching when:
- Task scope is ambiguous (could be risky-architecture or could be compile-heavy-rust).
- Task touches both rendering and physics (high risk of silent desync).
- The Manager hasn't queued similar work recently and the task is your own decomposition.

Escalate AFTER spawning when:
- Picker stalls (per `Maestro.md` reflection rule — twice failing same way).
- Tests pass but BRP smoke shows visual artifact.
- Cargo bench shows regression on a path you didn't expect to be hot.
</escalation_rules>

<related>
- `task-types/compile-heavy-rust.md` — parent task class.
- `harnesses/claude-code.md` — preferred harness.
- `feat-sandboxing-profile-x-backend` — hardened × docker setup.
- `projects/blueberry/sdf-art-policy.md` — when SDF skill applies.
- `projects/blueberry/code-conventions.md` — full invariant list.
</related>
