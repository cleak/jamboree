---
id: task-trace-replay-view
type: task
status: done
created: 2026-05-04T03:59:58.469839702Z
updated: 2026-05-06T13:41:02.675961530Z
edges:
- target: feat-ui-server
  type: child_of
---
Phase 6 (§12). Trace replay view (show full chain backwards from a Picker / decision).

Per §18.4.

Acceptance: trace replay shows complete chain from current view backwards to root trigger.

Implementation note (2026-05-06): `jam-ui-server` now exposes authenticated
`GET /api/trace/{trace_id}?token=...&max_depth=...`, backed by durable
`$JAM_HOME/journal/**/journal.*.jsonl` replay. `ui/src/main.tsx` links trace
rows to `/traces/<trace-id>` and renders the child-to-root chain plus
chronological entries, actors, parents, source file/line, and payloads. Smoke
served the built UI through `jam-ui-server` with temporary NATS and a temporary
journal; `/traces/<child>` returned the SPA shell and `/api/trace/<child>`
returned `["child", "root"]` with the expected Maestro and Picker entries.
