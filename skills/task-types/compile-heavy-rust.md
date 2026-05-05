---
scope: task-types/compile-heavy-rust
---

# Task Type — Compile-Heavy Rust

Tasks where Bevy compile time dominates wall-clock per task. Includes most non-trivial Blueberry refactors and architectural changes.

<concurrency_cap>
Per spec §6.7 for Blueberry: **3 concurrent compile-heavy-rust Pickers global cap**.

The global concurrent-Picker cap is 8 (per `metric-pickers-concurrent-cap-global`); only 3 of those slots can be compile-heavy at once. Substrate enforces this in `spawn-picker`.

If the cap is full when you want to dispatch, queue the task or downgrade to a lighter task class if possible.
</concurrency_cap>

<sandbox_profile>
Default: `default × local`. Shared `target/` symlink for `sccache` benefit.

For long unattended runs (overnight), prefer `default × docker` per spec §6.5 build cache strategy:
- Per-Picker `target/` mounted from a shared volume (read-only base + per-task overlay).
- `sccache` configured.
- Mold linker.
- Incremental compilation enabled by default in `~/.jam/config/build.toml`.

Hardened profile (`hardened × docker`) only when a task crosses into risky-architecture (`task-types/risky-architecture.md`).
</sandbox_profile>

<harness_selection>
Best fit:
- **Codex CLI** during business hours when 5h windows align with your work cadence. Speed-mode optional.
- **Claude Code** when extended thinking helps (cross-system refactors, ECS architecture changes).
- **OpenCode + DeepSeek** for overnight batch (compile time dominates wall-clock anyway; cost wins).

Avoid:
- Light harnesses (Aider) for compile-heavy work — context limits and tool-call discipline are wrong fit.
</harness_selection>

<budget_per_picker>
Suggested ranges:
- Small refactor (1-3 files): $3 - $8.
- Medium refactor (1 crate): $8 - $20.
- Cross-crate refactor: $15 - $40 (consider Claude Code with extended thinking).

Set `budget_usd` explicitly in the spawn spec. The Picker hard-aborts at 125% of its own budget; the Maestro hard-aborts at 125% of its session budget.
</budget_per_picker>

<spawn_template>
```
spawn-picker(spec={
    task_id: "...",
    harness: "codex-cli" | "claude-code" | "opencode",
    sandbox_backend: "local" | "docker",
    sandbox_profile: "default",
    task_class: "compile-heavy-rust",
    initial_prompt: """
        <task description>

        Project context:
        - Blueberry / Bevy 0.18 voxel game.
        - Trust the codebase over web-search for Bevy 0.18 APIs.
        - Apply commit and PR validation gates per skills/projects/blueberry/commit-validation.md.

        If the change touches a hot path (per skills/projects/blueberry/hot-paths.md):
        - Run cargo bench before and after.
        - If >1% regression, revert and report.

        Acceptance:
        - cargo fmt --check passes.
        - cargo clippy --workspace --all-targets -- -D warnings passes.
        - cargo test --workspace passes.
        - PR opened (ready-for-review, not draft) via gh pr create.
    """,
    budget_usd: 8.00 - 40.00,
})
```
</spawn_template>

<expected_wall_clock>
For Blueberry on a Ryzen 7 6800H (Caleb's machine):
- Single-crate refactor: 15-45 min wall-clock.
- Cross-crate refactor: 45-120 min.
- Whole-workspace touch: 90-180 min.

Most of this is `cargo build` and `cargo test` — actual model time is small. Overnight runs are economical because human waiting is zero.
</expected_wall_clock>

<related>
- `harnesses/codex-cli.md`, `harnesses/claude-code.md`, `harnesses/opencode-deepseek.md` — harness selection.
- `projects/blueberry/commit-validation.md` — gates the Picker must satisfy.
- `projects/blueberry/hot-paths.md` — paths where extra benchmarking is required.
- `task-types/ecs-refactor.md` — sub-class for ECS-specific refactors.
- `task-types/risky-architecture.md` — when to upgrade to hardened sandbox.
</related>
