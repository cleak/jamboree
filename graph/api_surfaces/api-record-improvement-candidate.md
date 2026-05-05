---
id: api-record-improvement-candidate
type: api_surface
status: draft
created: 2026-05-04T03:53:19.374048623Z
updated: 2026-05-04T04:33:11.336679244Z
edges:
- target: feat-maestro-tool-surface
  type: exposed_by
---
`record-improvement-candidate(category, description, motivation)` (§5.8, §7.2 Tier 2). Flags a potential system change for human review.

Never applied automatically. Queue at `~/.jam/improvement-candidates.jsonl` (or similar — TBD based on review workflow).