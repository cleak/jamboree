---
id: api-find-traces
type: api_surface
status: stable
created: 2026-05-04T03:53:12.070686041Z
updated: 2026-05-06T21:54:08Z
edges:
- target: comp-trace-replay-tool
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`find-traces(filter, limit?)` → search traces matching pattern (§5.8). E.g., `harness=codex-cli AND outcome=failed AND since=last-7d`.

Implementation note (2026-05-06): `jam_ui_server::trace_replay::find_traces_in_journal` reads `$JAM_HOME/journal/**/journal.*.jsonl`, groups entries by `trace_id`, and applies `AND` filters over envelope fields (`event`, `event_type`, `actor`, trace fields), scalar payload fields (`task_id`, `session_id`, `pr_ref`, `harness`/`harness_id`, `outcome`, etc.), and `since=` cutoffs (`last-Nd`, `last-Nh`, `last-Nm`, RFC 3339, or `YYYY-MM-DD`). The surface is exposed by `jam trace find <filter> --limit N` and the authenticated UI endpoint `GET /api/traces/find?filter=...&limit=N`.
