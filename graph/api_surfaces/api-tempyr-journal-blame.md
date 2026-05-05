---
id: api-tempyr-journal-blame
type: api_surface
status: draft
created: 2026-05-04T03:52:46.940240223Z
updated: 2026-05-04T04:56:59.225609187Z
edges:
- target: comp-jam-svc-knowledge
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`tempyr-journal-blame(file-path)` → wraps Tempyr's `journal_blame` (§5.5, §22.5).

"What entries referenced this file?" Useful for Maestro planning a new task — find prior work and dead ends in this code area.