---
id: api-inspect-picker
type: api_surface
status: draft
created: 2026-05-04T03:52:03.912976754Z
updated: 2026-05-04T04:54:10.549258922Z
edges:
- target: comp-jam-svc-session
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`inspect-picker(handle)` → `PickerStatus` (§5.2). Wraps the harness adapter's inspect method.