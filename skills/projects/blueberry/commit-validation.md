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

Before exiting (the coordinator opens the PR — see "Opening the PR" below):
- [ ] `cargo check --workspace` passes.
- [ ] `cargo test --workspace` passes.
- [ ] At least one commit ahead of trunk on `task/<task-id>`.
- [ ] Working tree is clean (no uncommitted changes outside `.jam/`).
- [ ] `.jam/pr-title.txt` written — single line, conventional prefix, ≤240 chars.
- [ ] `.jam/pr-body.md` written — Summary + Verification sections.

## What NOT to do

- Don't run `cargo fmt` on `.wgsl` shader files — they're not Rust.
- Don't bypass clippy with broad `#[allow(...)]`. Targeted allows with comment-justification only.
- Don't open draft PRs by default. Only when the Manager explicitly asks for one.
- Don't run git operations on the same worktree in parallel — concurrent commands cause `index.lock` contention. The orchestrator's worktree creation protocol prevents this between Pickers; within a Picker, serialize git calls.
- Don't commit secrets. The orchestrator's journal writer redacts known secret patterns at write time, but commits are unfiltered. Keep `gitleaks` clean.

## Opening the PR (handled by the orchestrator)

**You do not push the branch. You do not run `gh pr create`. You do not call `open-pr` yourself.** The post-picker coordinator (`jam-task-lifecycle`) detects `picker.exited` with `exit_code=0`, runs pre-checks on your worktree, and calls `tool.repo.open-pr` itself. See `graph/decisions/dec-post-picker-coordination.md`.

Your contract as a Picker — *exactly* this, no more, no less:

1. Make the code changes the task requires.
2. Commit them on the task branch (`task/<task-id>`, already checked out). One commit is fine; multiple commits are fine.
3. Write the PR title to `.jam/pr-title.txt` — a single line, conventional-commit format (`feat:`, `fix:`, `refactor:`, etc.). 240 chars max.
4. Write the PR body to `.jam/pr-body.md` — Markdown, with **Summary** and **Verification** sections at minimum. Reference the issue/task by id where relevant.
5. Leave the working tree clean. Uncommitted changes *outside* `.jam/` will cause the coordinator to reject the worktree and resume your session asking you to clean up.
6. Exit normally (`exit_code=0`).

If you exit non-zero, exit with uncommitted changes, exit with no commits ahead of trunk, or exit without `.jam/pr-title.txt`/`.jam/pr-body.md`, the coordinator emits `picker.continuation-needed` and your session is resumed with a prompt explaining what to fix. Treat that resume prompt as authoritative — don't second-guess; address the specific deficiency it names.

When CodeRabbit comments or CI failures arrive on the opened PR, the same coordinator emits `picker.continuation-needed` with the comments / failure log and resumes your session. You'll see the new input as a fresh user prompt in the same conversation — read the original task description back in your context if you need to re-orient.

Branches and pushes flow through the orchestrator's user-to-server GitHub token so PRs are attributed to a real user (`is_bot:false`) and reviewer bots auto-review. **Pushing manually breaks that attribution.**
