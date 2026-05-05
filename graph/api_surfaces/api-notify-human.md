---
id: api-notify-human
type: api_surface
status: draft
created: 2026-05-04T03:53:26.713883590Z
updated: 2026-05-04T04:58:29.090374581Z
edges:
- target: comp-jam-svc-supervise
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`notify-human(urgency, summary, payload?)` → triggers ntfy push; surfaced in UI (§5.8).

Urgency levels: low (FYI), medium (default), high (notable), critical (immediate attention).

Topic name: `jam-<user-id>-<install-id>`. Token-protected. iOS/Android ntfy app for delivery.