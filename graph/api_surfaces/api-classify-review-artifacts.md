---
id: api-classify-review-artifacts
type: api_surface
status: stable
created: 2026-05-04T03:51:55.378556962Z
updated: 2026-05-06T19:40:00Z
edges:
- target: comp-jam-svc-observe
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`classify-review-artifacts(artifacts)` → applies LLM classifier (cheap model) for kind/intent (§5.1, §4.2.2).

Wraps per-reviewer adapter classifications and applies a normalization pass.

The Maestro request contract is generated as
`ObserveClassifyReviewArtifactsRequest` and routed to
`tool.observe.classify-review-artifacts`.

Implementation note (2026-05-06): `jam-svc-observe` now serves this route.
The response returns traced classifications for each input artifact and keeps
outside-authored bodies marked as `body_trust: untrusted`; prompt-injection
phrases are classified as `suspicious-prompt-injection`.
