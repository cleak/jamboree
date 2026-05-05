---
id: feat-event-schema-versioning
type: feature
status: draft
created: 2026-05-04T03:28:25.262498626Z
updated: 2026-05-04T04:39:41.147268768Z
owner: caleb
edges:
- target: api-events-toml-manifest
  type: exposes
- target: comp-events-toml-and-codegen
  type: uses
- target: comp-jam-events-crate
  type: uses
- target: comp-orchestrator-jsonl-journal
  type: uses
- target: dec-events-toml-manifest
  type: depends_on
- target: jamboree-v5
  type: child_of
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-journal-is-sacred-no-compaction
  type: constrained_by
- target: task-pydantic-typed-tool-io-codegen
  type: parent_of
---
Single shared `events.toml` manifest declares current versions for every event-emitting service. Build-time codegen generates per-event Rust types from the manifest, ensuring the version-bump conversation happens at edit time (§4.4.3).

Rules:
- Additive (new optional field): bump `event_subtype_version`. Old consumers ignore unknown fields. Serde `default` handles missing fields when reading old events.
- Breaking (remove/change semantics): introduce a new event type (e.g., `picker.spawned.v2`). Old type stays in journal forever; new code emits new type. Reconciler reads both. Eventually deprecate; never delete journal data.
- **No compaction** (per `principle-journal-is-sacred-no-compaction`).

Codegen output:
- `crates/jam-events/src/generated/types.rs` — Rust structs with serde derives.
- `crates/jam-events/src/generated/schemas/<event-type>.json` — JSON Schema files.
- `maestro/src/jam_maestro/events/_generated.py` — Pydantic models.

`tools/events-codegen.py` runs as Cargo build script and pre-commit hook. CI verifies generated files in sync with `events.toml`.