---
id: task-events-codegen-pipeline
type: task
status: backlog
created: 2026-05-04T03:57:55.481906394Z
updated: 2026-05-04T04:08:39.388441065Z
edges:
- target: feat-substrate-services
  type: child_of
---
Phase 0 (§12). Implement `tools/events-codegen.py` end-to-end: read `events.toml` → emit Rust types (`crates/jam-events/src/generated/types.rs`), JSON Schema files, Pydantic models (`maestro/src/jam_maestro/events/_generated.py`).

Wire as Cargo build script + pre-commit hook. CI verifies generated files in sync.

Per `comp-events-toml-and-codegen`.

Acceptance: edit events.toml → codegen → Rust types and Pydantic models update consistently.