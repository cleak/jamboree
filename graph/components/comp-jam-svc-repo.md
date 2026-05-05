---
id: comp-jam-svc-repo
type: component
status: planned
created: 2026-05-04T03:39:33.056690005Z
updated: 2026-05-04T04:56:05.701609992Z
edges:
- target: api-mark-review-artifact-handled
  type: exposes
- target: api-open-pr
  type: exposes
- target: api-pr-status
  type: exposes
- target: api-prepare-merge
  type: exposes
- target: api-read-pr-comments
  type: exposes
- target: api-reply-to-comment
  type: exposes
- target: api-request-human-merge
  type: exposes
- target: api-request-review
  type: exposes
- target: comp-events-toml-and-codegen
  type: depends_on
- target: comp-jam-trace-crate
  type: depends_on
- target: comp-nats-jetstream
  type: depends_on
- target: comp-routing-manifest
  type: depends_on
- target: feat-maestro-tool-surface
  type: used_by
- target: feat-tool-services-out-of-process
  type: used_by
- target: principle-decoupled-processes-bus
  type: constrained_by
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
---
Repo / PR ops tool service. Subject prefix `tool.repo.*`. Crate `crates/jam-svc-repo/`.

Tools (§5.4):
- `open-pr(branch, title, body, draft?)` → `PullRequestRef`
- `pr-status(pr-ref)` → typed PR state
- `read-pr-comments(pr-ref)` → `Vec<ReviewArtifact>`
- `reply-to-comment(artifact-id, text)` → posts reply via reviewer adapter
- `mark-review-artifact-handled(artifact-id, status, reasoning)`
- `request-review(pr-ref, reviewer-id)`
- `prepare-merge(pr-ref)` — final pre-merge checks, doesn't merge
- `request-human-merge(pr-ref, summary)` — notifies human via ntfy and UI; **only path to merge**

Reviewer adapter implementations and GitHub App client live here.