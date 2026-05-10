---
id: api-read-pr-comments
type: api_surface
status: stable
created: 2026-05-04T03:52:22.843179770Z
updated: 2026-05-06T21:26:08Z
edges:
- target: comp-jam-svc-repo
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`read-pr-comments(pr-ref)` → `Vec<ReviewArtifact>` (§5.4). Each artifact's `body` is `Untrusted<String>`.

The Maestro request contract is generated as `RepoReadPrCommentsRequest` and
routed to `tool.repo.read-pr-comments`.

Implementation note (2026-05-06): `jam-svc-repo` now implements the request on the current `gh api` fallback backend. It returns GitHub issue comments, PR review comments, and PR reviews as normalized review artifacts with stable IDs and `body_trust: untrusted`; the service wraps each body in the Rust `Untrusted<String>` newtype before exposing it as JSON data.
