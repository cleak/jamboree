---
id: api-request-review
type: api_surface
status: stable
created: 2026-05-04T03:52:29.559549702Z
updated: 2026-05-06T21:26:08Z
edges:
- target: comp-jam-svc-repo
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`request-review(pr-ref, reviewer-id)` → triggers a specific reviewer (§5.4).

Implementation note (2026-05-06): the Maestro registry now routes `request-review` to `tool.repo.request-review` with `RepoRequestReviewRequest`. `jam-svc-repo` implements the local `codex-review` reviewer ID through the installed `codex review` subcommand, requiring a native Linux `worktree_path` so the service cannot review the wrong checkout.
