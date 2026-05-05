---
id: dec-events-toml-manifest
type: decision
status: decided
created: 2026-05-04T03:46:26.364772210Z
updated: 2026-05-04T05:03:03.820455581Z
edges:
- target: comp-events-toml-and-codegen
  type: decision_for
- target: feat-event-schema-versioning
  type: depended_on_by
---
**Single shared `events.toml` manifest + codegen** (§4.4.3). Forces the version-bump conversation at edit time. Build-time codegen generates per-event Rust types, JSON Schema files, Pydantic models.

Why over per-event `version` field: a manifest forces the conversation; codegen ensures producers and consumers stay in sync. A pre-commit hook regenerates and verifies; CI re-checks.

Mitigation against §13.16 (codegen drift): consumers fail loudly on unknown event types or missing required fields rather than silently mis-parsing.