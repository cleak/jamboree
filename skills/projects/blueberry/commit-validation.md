---
scope: blueberry/commit
---

# Blueberry — Commit and PR Validation Gates

Blueberry has explicit pre-commit and pre-PR validation gates. **Pickers must run these before committing or opening a PR.** Skipping or "I'll fix the warnings later" is not acceptable.

Source: `/home/caleb/blueberry/CLAUDE.md` (Commit Validation, PR Creation Validation sections).

## Pre-commit (every commit, including checkpoint/WIP)

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```

Both must pass. The `--workspace --all-targets` flags ensure sub-crates (`blueberry_jobs`, `blueberry_terrain_foundation`) and their tests/benches are also linted; without them, warnings inside `crates/*` slip through.

If `cargo fmt --check` fails: run `cargo fmt` and re-stage.

If `cargo clippy` fails: fix the warning. Don't `#[allow(...)]` to bypass unless you have a specific justification you'd defend in code review.

## Pre-PR (when opening a new PR)

All four must pass:

```bash
cargo fmt --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

PR is ready-for-review by default — do NOT open as draft unless explicitly asked.

## Conventional commit prefixes

Imperative, scoped summaries:
- `feat:` — new functionality.
- `fix:` — bug fix.
- `refactor:` — internal restructuring without functional change.
- `bump:` — dependency or version bump.
- `tune:` — parameter / threshold / heuristic adjustment.

Examples:
- `fix: stabilize moebius highlights`
- `feat: add canyon spline seam protocol`
- `refactor: extract canyon generator from terrain meshing`

Include what changed and why in the commit body.

## Tests are mandatory

- Unit tests are mandatory for all new code.
- Keep tests close to code using `#[cfg(test)] mod tests` in each module.
- Name tests by behavior: `camera_has_required_components`, NOT `test_camera`.
- Add a regression test with every bug fix.
- Test systems in isolation where possible.

## Pre-commit checklist for Pickers

Before any commit:
- [ ] `cargo fmt --check` passes.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes.
- [ ] Conventional prefix on commit message.
- [ ] Tests added for new code (and regressions for bug fixes).
- [ ] Journal logged: `plan`, `finding`s, any `decision`s, final `outcome`.

Before opening a PR:
- [ ] `cargo check --workspace` passes.
- [ ] `cargo test --workspace` passes.
- [ ] PR title uses conventional prefix.
- [ ] PR body explains what changed and why.
- [ ] PR is ready-for-review (not draft) unless asked otherwise.

## What NOT to do

- Don't run `cargo fmt` on `.wgsl` shader files — they're not Rust.
- Don't bypass clippy with broad `#[allow(...)]`. Targeted allows with comment-justification only.
- Don't open draft PRs by default. Only when the Manager explicitly asks for one.
- Don't run git operations on the same worktree in parallel — concurrent commands cause `index.lock` contention. The orchestrator's worktree creation protocol prevents this between Pickers; within a Picker, serialize git calls.
- Don't commit secrets. The orchestrator's journal writer redacts known secret patterns at write time, but commits are unfiltered. Keep `gitleaks` clean.

## After commit

Don't push immediately if you're going to keep working — the worktree's branch will accumulate commits as you iterate. When ready, push and open the PR via `gh pr create` (or the orchestrator's `open-pr` tool, which uses `octocrab`).

The PR creation flow auto-emits `pr.opened` to NATS, which propagates to `task-lifecycle-handler` and updates the Tempyr task node to `status: in-review`.
