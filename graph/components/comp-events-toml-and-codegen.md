---
id: comp-events-toml-and-codegen
type: component
status: active
created: 2026-05-04T03:31:37.804286873Z
updated: 2026-05-06T21:15:00Z
edges:
- target: api-events-toml-manifest
  type: exposes
- target: comp-jam-svc-evolve
  type: depended_on_by
- target: comp-jam-svc-knowledge
  type: depended_on_by
- target: comp-jam-svc-message
  type: depended_on_by
- target: comp-jam-svc-observe
  type: depended_on_by
- target: comp-jam-svc-repo
  type: depended_on_by
- target: comp-jam-svc-research
  type: depended_on_by
- target: comp-jam-svc-search
  type: depended_on_by
- target: comp-jam-svc-session
  type: depended_on_by
- target: comp-jam-svc-supervise
  type: depended_on_by
- target: comp-jam-svc-worktree
  type: depended_on_by
- target: comp-pre-commit-hooks
  type: depended_on_by
- target: dec-events-toml-manifest
  type: has_decision
- target: feat-event-schema-versioning
  type: used_by
- target: feat-substrate-services
  type: used_by
- target: feat-tool-services-out-of-process
  type: used_by
- target: principle-journal-is-sacred-no-compaction
  type: constrained_by
---
Single shared `events.toml` manifest at `crates/jam-events/events.toml` (§4.4.3). Build-time codegen generates per-event Rust types from the manifest.

Codegen output:
- `crates/jam-events/src/generated/types.rs`
- `crates/jam-events/src/generated/schemas/<event-type>.json`
- `maestro/src/jam_maestro/events/_generated.py`

`tools/events-codegen.py` runs as Cargo build script and pre-commit hook. CI verifies generated files in sync with `events.toml` (§13.16).

Tool I/O codegen follows the same contract-first shape. The Phase 1 slice keeps checked-in JSON schemas at `crates/jam-tools-core/schemas/<service>/<tool>.<request|response>.json` for observe, repo, session, and worktree, then `tools/pydantic-gen.py` renders generated Pydantic models under `maestro/src/jam_maestro/tools/` (§11.2.6).

Current Maestro usage wires `ObserveClient.world_snapshot` through `ObserveWorldSnapshotRequest`, giving pyright a concrete generated type boundary. Future hardening should move the schema source from checked-in JSON to Rust `schemars` exports without changing the generated Python package shape.
