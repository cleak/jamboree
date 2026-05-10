---
id: api-tempyr-journal-search
type: api_surface
status: stable
created: 2026-05-04T03:52:44.633734335Z
updated: 2026-05-06T22:06:41Z
edges:
- target: comp-jam-svc-knowledge
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`tempyr-journal-search(query, kind?, agent?, since?, limit?)` → wraps Tempyr's `journal_search` (§5.5, §22.5).

Hybrid retrieval: BM25 + vec0 vector search + RRF (reciprocal rank fusion) + recency weighting + kind boost.

Use cases: Maestro woken by `picker.errored` searches recent `dead_end` entries from the same agent; skill-suspicion-reconciler queries `dead_end` corpus for skill tag accumulation.

Implementation note (2026-05-06): the Maestro-local `meta.tempyr-journal-search` wrapper in `jam_maestro.tempyr_journal_query` invokes `tempyr journal search --json` without a shell, bounds `limit`/`token_budget`, supports repeated `kind`, `since_days`, and post-filters by `agent` when requested. It normalizes `hits[]` into typed `TempyrJournalHit` records.
