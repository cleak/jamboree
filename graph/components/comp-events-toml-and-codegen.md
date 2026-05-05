---
id: comp-events-toml-and-codegen
type: component
status: planned
created: 2026-05-04T03:31:37.804286873Z
updated: 2026-05-04T05:03:03.820455044Z
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

Same approach for tool I/O: each `jam-svc-*` crate's Rust types (with `schemars` derive) → `<service>.schema.json` via `tools/schema-export.rs` → Pydantic models via `tools/pydantic-gen.py` → `maestro/src/jam_maestro/tools/<service>.py` (§11.2.6).