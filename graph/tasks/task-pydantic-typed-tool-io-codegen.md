---
id: task-pydantic-typed-tool-io-codegen
type: task
status: done
created: 2026-05-04T03:58:28.676275500Z
updated: 2026-05-06T06:00:18.791065354Z
edges:
- target: feat-event-schema-versioning
  type: child_of
---
Phase 1 (§12). Pydantic-typed tool I/O via codegen. Each `jam-svc-*` crate exposes its tool I/O as JSON schema; build-time script generates Pydantic models.

Per `comp-events-toml-and-codegen` (extended for tool schemas), `dec-events-toml-manifest`.

Acceptance: pyright catches "Maestro passed `task-id` to a tool that expects `task_id`" at type-check time.

Implementation note 2026-05-06:
- Added `crates/jam-tools-core/schemas/<service>/<tool>.<request|response>.json` for the current Phase 1 tool contracts: observe, repo, session, and worktree.
- Added `tools/pydantic-gen.py`, which renders generated Pydantic models under `maestro/src/jam_maestro/tools/` and supports `--check` drift detection.
- Wired `ObserveClient.world_snapshot` to require `ObserveWorldSnapshotRequest`, so raw dict calls are rejected by pyright before runtime.
- Verified with a negative pyright smoke: `{"task-id": "task-1"}` is not assignable to `ObserveWorldSnapshotRequest`.
