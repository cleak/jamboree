## Summary

Appends the requested e2e sanity comment to `crates/jam-tools-core/src/lib.rs` so the Maestro-launched Picker path can verify an end-to-end task branch, commit, and PR metadata flow.

## Verification

- `cargo check -p jam-tools-core` - passed
