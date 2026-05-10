---
id: api-tempyr-journal-blame
type: api_surface
status: stable
created: 2026-05-04T03:52:46.940240223Z
updated: 2026-05-06T22:06:41Z
edges:
- target: comp-jam-svc-knowledge
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`tempyr-journal-blame(file-path)` → wraps Tempyr's `journal_blame` (§5.5, §22.5).

"What entries referenced this file?" Useful for Maestro planning a new task — find prior work and dead ends in this code area.

Implementation note (2026-05-06): the Maestro-local `meta.tempyr-journal-blame` wrapper in `jam_maestro.tempyr_journal_query` invokes `tempyr journal blame --json` without a shell, bounds `limit`/`token_budget`, supports repeated `kind`, and normalizes the resulting `hits[]` records.
