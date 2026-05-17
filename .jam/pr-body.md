## Summary

- Adds a documented no-op selftest marker constant in `jam-svc-worktree`.
- Covers the marker with a unit assertion so the self-modification pipeline has a visible, harmless code delta.
- Fixes post-picker PR handoff so the repo and base branch are resolved from the Picker worktree, preserving Blueberry routing while using `cleak/jamboree` and `main` for Jamboree's main-only repo.
- Leaves Blueberry/Jamboree worktree target selection behavior unchanged.

## Verification

- `cargo fmt --check` - failed because this toolchain has no `cargo-fmt` subcommand installed.
- `rustfmt --check crates/jam-svc-worktree/src/main.rs` - failed because direct `rustfmt` defaulted to Rust 2015 without Cargo metadata.
- `rustfmt --edition 2021 --check crates/jam-svc-worktree/src/main.rs crates/jam-task-lifecycle/src/post_picker.rs` - passed.
- `cargo test -p jam-svc-worktree` - passed, 10 tests.
- `cargo test -p jam-svc-worktree -p jam-task-lifecycle` - passed, 24 tests.
- `cargo clippy -p jam-svc-worktree --all-targets -- -D warnings` - passed.
- `cargo clippy -p jam-svc-worktree -p jam-task-lifecycle --all-targets -- -D warnings` - passed.
- `cargo build -p jam-svc-worktree` - passed.
- `cargo build -p jam-svc-worktree -p jam-task-lifecycle` - passed.

## Deploy

- `/opt/jam/bin/jam deploy task-lifecycle` - passed; patch confirmed with 1 check.
- `jam deploy worktree` - not run; `jam-svc-worktree` runtime deployment was not needed to unblock PR handoff.
