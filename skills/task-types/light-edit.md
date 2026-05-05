---
scope: task-types/light-edit
---

# Task Type — Light Edit

Small, bounded edits: one or two file changes, < 100 lines of diff, no architectural impact. Most CodeRabbit suggestions, single bug fixes, small test additions, comment cleanups.

<concurrency_cap>
8 concurrent globally (shares the doc-generation / shader-variant slot per spec §6.7).
</concurrency_cap>

<sandbox_profile>
`default × local`. No need for hardening on routine small changes.
</sandbox_profile>

<harness_selection>
**Codex CLI** is the typical default — cheap (subscription), fast, good tool-call discipline for narrow edits.

**OpenCode + DeepSeek Flash** as a cost-floor option when Codex quota is constrained.

Avoid Claude Code for routine light-edits — burns Pro/Max quota for work that doesn't need extended reasoning.
</harness_selection>

<budget>
Suggested ranges:
- Simple bug fix or test addition: $0.50 - $2.
- Small refactor (rename, extract): $1 - $5.

Don't over-budget. If the work seems to need >$5, it probably isn't a light-edit — bump to compile-heavy-rust or ecs-refactor.
</budget>

<what_qualifies>
- CodeRabbit / codex-review suggestion that touches 1-2 files.
- Adding a regression test for a known bug.
- Renaming a function or symbol within one crate.
- Updating a docstring or comment.
- Fixing a clippy warning.
- Adding a `#[allow(...)]` with comment-justification (when warranted).

Doesn't qualify:
- Multi-file refactors → `compile-heavy-rust` or `ecs-refactor`.
- New systems or plugins → `ecs-refactor` or `risky-architecture`.
- Changes to project conventions or shared invariants → `risky-architecture`.
</what_qualifies>

<spawn_template>
```
spawn-picker(spec={
    task_id: "...",
    harness: "codex-cli",
    sandbox_backend: "local",
    sandbox_profile: "default",
    task_class: "light-edit",
    initial_prompt: """
        Light edit: <description>

        Scope: 1-2 files, minimal diff.

        Project: Blueberry. Apply commit gates per skills/projects/blueberry/commit-validation.md.

        Acceptance:
        - cargo fmt --check + cargo clippy --workspace --all-targets -- -D warnings.
        - Existing tests pass.
        - PR opened ready-for-review.
    """,
    budget_usd: 0.50 - 5.00,
})
```
</spawn_template>

<promotion_rule>
If the Picker reports "the change is bigger than expected", **stop the Picker via `interrupt-with-message`, mark the original task `purge-session(handle, reason="upgraded to compile-heavy-rust")`**, and dispatch a new Picker with the higher task class.

Don't let a light-edit Picker drift into a substantial change — budget will be wrong, sandbox profile may be wrong, and concurrency caps may be wrong.
</promotion_rule>

<related>
- `task-types/coderabbit-review.md` — most common dispatcher of light-edits.
- `task-types/compile-heavy-rust.md` — promote to here when scope grows.
- `harnesses/codex-cli.md` — typical harness for light-edits.
</related>
