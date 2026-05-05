---
id: task-pydantic-typed-tool-io-codegen
type: task
status: backlog
created: 2026-05-04T03:58:28.676275500Z
updated: 2026-05-04T04:10:10.960885085Z
edges:
- target: feat-event-schema-versioning
  type: child_of
---
Phase 1 (§12). Pydantic-typed tool I/O via codegen. Each `jam-svc-*` crate exposes its tool I/O as JSON schema; build-time script generates Pydantic models.

Per `comp-events-toml-and-codegen` (extended for tool schemas), `dec-events-toml-manifest`.

Acceptance: pyright catches "Maestro passed `task-id` to a tool that expects `task_id`" at type-check time.