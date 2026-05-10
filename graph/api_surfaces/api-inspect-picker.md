---
id: api-inspect-picker
type: api_surface
status: stable
created: 2026-05-04T03:52:03.912976754Z
updated: 2026-05-06T21:26:08Z
edges:
- target: comp-jam-svc-session
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`inspect-picker(handle)` → `PickerStatus` (§5.2). Wraps the harness adapter's inspect method.

Implementation note (2026-05-06): `tool.session.inspect-picker` is implemented in `jam-svc-session` and exposed through `MaestroToolRegistry` with typed `session_id` validation.
