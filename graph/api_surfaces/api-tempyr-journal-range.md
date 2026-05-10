---
id: api-tempyr-journal-range
type: api_surface
status: stable
created: 2026-05-04T03:52:49.256080520Z
updated: 2026-05-06T22:06:41Z
edges:
- target: comp-jam-svc-knowledge
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`tempyr-journal-range(rev-range)` → wraps Tempyr's `journal_range` (§5.5, §22.5).

"What did agents reason about during this span of git history?"

Implementation note (2026-05-06): the Maestro-local `meta.tempyr-journal-range` wrapper in `jam_maestro.tempyr_journal_query` invokes `tempyr journal range --json` without a shell, bounds `limit`/`token_budget`, supports repeated `kind`, and normalizes the resulting `hits[]` records.
