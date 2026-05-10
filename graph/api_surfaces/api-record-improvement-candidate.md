---
id: api-record-improvement-candidate
type: api_surface
status: stable
created: 2026-05-04T03:53:19.374048623Z
updated: 2026-05-06T21:34:43Z
edges:
- target: feat-maestro-tool-surface
  type: exposed_by
---
`record-improvement-candidate(category, description, motivation)` (§5.8, §7.2 Tier 2). Flags a potential system change for human review.

Never applied automatically. Queue at `~/.jam/improvement-candidates.jsonl` (or similar — TBD based on review workflow).

Implementation note (2026-05-06): `record-improvement-candidate` is a local Maestro meta-tool routed as `meta.record-improvement-candidate`. It appends structured JSONL records to `$JAM_HOME/improvement-candidates.jsonl` for human review.
