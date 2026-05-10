---
id: api-read-journal
type: api_surface
status: stable
created: 2026-05-04T03:53:14.490843123Z
updated: 2026-05-06T21:37:34Z
edges:
- target: feat-maestro-tool-surface
  type: exposed_by
---
`read-journal(filters)` → query journal directly (§5.8). Rare; usually `query-session-store` is better for full-text.

Implementation note (2026-05-06): `read-journal` is a local Maestro meta-tool routed as `meta.read-journal`. It reads rotated JSONL under `$JAM_HOME/journal`, supports bounded filters for `trace_id`, `event_type`, `task_id`, `session_id`, and `pr_ref`, and fails loudly on malformed journal JSON.
