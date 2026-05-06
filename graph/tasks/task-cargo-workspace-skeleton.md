---
id: task-cargo-workspace-skeleton
type: task
status: done
created: 2026-05-04T03:57:50.401141428Z
updated: 2026-05-06T02:51:31.249789046Z
edges:
- target: feat-substrate-services
  type: child_of
---
Phase 0 (§12). Set up the Cargo workspace with all `crates/jam-*` skeleton crates created (empty `lib.rs` / `main.rs`). Per `comp-monorepo-tree`.

Acceptance: `cargo build --workspace` succeeds on a fresh checkout.