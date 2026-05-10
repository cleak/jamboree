---
id: task-patch-event-vocabulary
type: task
status: done
created: 2026-05-04T04:00:21.911579441Z
updated: 2026-05-06T08:17:46Z
edges:
- target: feat-hot-patching
  type: child_of
---
Phase 7 (§12). Patch event vocabulary: `patch.staged`, `patch.applied`, `patch.confirmed`, `patch.rolled-back`, `patch.failed`, `patch.lock-acquired`, `patch.lock-released`, `patch.rolled-back-successfully`.

Per §20.6.

Implementation note (2026-05-06): `crates/jam-events/events.toml` contains all eight patch event definitions listed above, and codegen has emitted Rust event types plus JSON schemas under `crates/jam-events/src/generated/`. Verified generated types include `PatchStaged`, `PatchApplied`, `PatchConfirmed`, `PatchRolledBack`, `PatchRolledBackSuccessfully`, `PatchFailed`, `PatchLockAcquired`, and `PatchLockReleased`; schemas include matching `patch.*.json` files.

Verification: `python3 tools/events-codegen.py --check`, `python3 tools/pydantic-gen.py --check`, and `cargo test -p jam-events`.
