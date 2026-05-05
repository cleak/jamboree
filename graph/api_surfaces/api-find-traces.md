---
id: api-find-traces
type: api_surface
status: draft
created: 2026-05-04T03:53:12.070686041Z
updated: 2026-05-04T04:59:05.510577283Z
edges:
- target: comp-trace-replay-tool
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`find-traces(filter)` → search traces matching pattern (§5.8). E.g., `harness=codex-cli AND outcome=failed AND since=last-7d`.