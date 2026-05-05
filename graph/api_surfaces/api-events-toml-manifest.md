---
id: api-events-toml-manifest
type: api_surface
status: draft
created: 2026-05-04T03:53:48.720408377Z
updated: 2026-05-04T04:59:32.687871764Z
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