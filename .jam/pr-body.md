## Summary

- Adds a documented no-op selftest marker constant in `jam-svc-worktree`.
- Covers the marker with a unit assertion so the self-modification pipeline has a visible, harmless code delta.
- Leaves Blueberry/Jamboree worktree target selection behavior unchanged.

## Verification

- `cargo fmt --check` - failed because this toolchain has no `cargo-fmt` subcommand installed.
- `rustfmt --check crates/jam-svc-worktree/src/main.rs` - failed because direct `rustfmt` defaulted to Rust 2015 without Cargo metadata.
- `rustfmt --edition 2021 --check crates/jam-svc-worktree/src/main.rs` - passed.
- `cargo test -p jam-svc-worktree` - passed, 10 tests.
- `cargo clippy -p jam-svc-worktree --all-targets -- -D warnings` - passed.
- `cargo build -p jam-svc-worktree` - passed.

## Deploy

- Not run; this task did not explicitly request a live deploy.
- Exact deploy command when approved: `jam deploy worktree`.
