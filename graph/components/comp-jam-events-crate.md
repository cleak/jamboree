---
id: comp-jam-events-crate
type: component
status: planned
created: 2026-05-04T03:39:46.883646051Z
updated: 2026-05-04T04:24:41.086437661Z
edges:
- target: feat-event-schema-versioning
  type: used_by
---
Crate `crates/jam-events/`. Holds the `events.toml` manifest (§4.4.3) and the codegen output:
- `crates/jam-events/src/generated/types.rs` — Rust structs with serde derives.
- `crates/jam-events/src/generated/schemas/<event-type>.json` — JSON Schema files.

Every event-emitting service uses these generated types. Pydantic side `maestro/src/jam_maestro/events/_generated.py` mirrors via `tools/events-codegen.py`.