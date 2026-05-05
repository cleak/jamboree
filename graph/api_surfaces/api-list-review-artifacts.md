---
id: api-list-review-artifacts
type: api_surface
status: draft
created: 2026-05-04T03:51:53.264181575Z
updated: 2026-05-04T04:53:26.270180620Z
edges:
- target: comp-jam-svc-observe
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`list-review-artifacts(pr-ref, status-filter?)` → `Vec<ReviewArtifact>` (§5.1, §4.2.4).

`ReviewArtifact.body` is `Untrusted<String>` — untrusted-content discipline (§11.2.4).