---
id: api-read-pr-comments
type: api_surface
status: draft
created: 2026-05-04T03:52:22.843179770Z
updated: 2026-05-04T04:55:20.786838577Z
edges:
- target: comp-jam-svc-repo
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`read-pr-comments(pr-ref)` → `Vec<ReviewArtifact>` (§5.4). Each artifact's `body` is `Untrusted<String>`.