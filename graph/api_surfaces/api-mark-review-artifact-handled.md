---
id: api-mark-review-artifact-handled
type: api_surface
status: stable
created: 2026-05-04T03:52:27.306509995Z
updated: 2026-05-06T21:26:08Z
edges:
- target: comp-jam-svc-repo
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`mark-review-artifact-handled(artifact-id, status, reasoning)` → updates internal status (§5.4).

Status: Open | Acknowledged | Addressed | Dismissed.

The Maestro request contract is generated as
`RepoMarkReviewArtifactHandledRequest` and routed to
`tool.repo.mark-review-artifact-handled`.

Implementation note (2026-05-06): `jam-svc-repo` now validates the closed status enum, appends local JSONL state under `JAM_REVIEW_ARTIFACT_STATE_PATH` / `$JAM_HOME/review-artifacts-handled.jsonl`, and publishes `journal.review-artifact.handled` with the caller's trace.
