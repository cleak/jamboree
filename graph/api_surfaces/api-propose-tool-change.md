---
id: api-propose-tool-change
type: api_surface
status: stable
created: 2026-05-04T03:53:24.258405200Z
updated: 2026-05-06T21:34:43Z
edges:
- target: feat-maestro-tool-surface
  type: exposed_by
---
`propose-tool-change(spec)` (§5.8, §7.2 Tier 3). For the Maestro to propose new tools or tool changes; queued for human review.

Implementation by human, not Maestro.

Implementation note (2026-05-06): `propose-tool-change` is a local Maestro meta-tool routed as `meta.propose-tool-change`. It appends structured JSONL records to `$JAM_HOME/tool-change-candidates.jsonl`; no runtime tool surface is changed automatically.
