---
id: api-events-toml-manifest
type: api_surface
status: stable
created: 2026-05-04T03:53:48.720408377Z
updated: 2026-05-06T21:29:02Z
edges:
- target: comp-events-toml-and-codegen
  type: exposed_by
- target: feat-event-schema-versioning
  type: exposed_by
---
The single source of truth for event shapes: `crates/jam-events/events.toml` (§4.4.3, `dec-events-toml-manifest`).

Codegen pipeline produces:
- Rust types (`schemars`-derived, serde) at `crates/jam-events/src/generated/types.rs`.
- JSON Schema at `crates/jam-events/src/generated/schemas/<event-type>.json`.
- Python Pydantic models at `maestro/src/jam_maestro/events/_generated.py`.

Pre-commit hook + CI verify in-sync with generated files.

Same approach for tool I/O: `<service>.schema.json` → Pydantic via `pydantic-gen.py` (§11.2.6).

Implementation note (2026-05-06): `tools/events-codegen.py` and `tools/pydantic-gen.py` are active codegen paths. Event schemas/types are generated from `crates/jam-events/events.toml`; tool request/response schemas under `crates/jam-tools-core/schemas/` generate Maestro Pydantic models.
