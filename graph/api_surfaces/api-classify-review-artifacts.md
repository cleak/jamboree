---
id: api-classify-review-artifacts
type: api_surface
status: draft
created: 2026-05-04T03:51:55.378556962Z
updated: 2026-05-04T04:53:35.345881833Z
edges:
- target: comp-jam-svc-observe
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`classify-review-artifacts(artifacts)` → applies LLM classifier (cheap model) for kind/intent (§5.1, §4.2.2).

Wraps per-reviewer adapter classifications and applies a normalization pass.