---
id: api-list-active-pickers
type: api_surface
status: stable
created: 2026-05-04T03:52:06.059203634Z
updated: 2026-05-06T21:26:08Z
edges:
- target: comp-jam-svc-session
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`list-active()` → all live Picker handles (§5.2).

Implementation note (2026-05-06): `tool.session.list-active` is implemented in `jam-svc-session` and exposed through `MaestroToolRegistry` with an empty typed request contract.
