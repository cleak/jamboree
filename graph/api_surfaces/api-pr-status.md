---
id: api-pr-status
type: api_surface
status: stable
created: 2026-05-04T03:52:20.626986789Z
updated: 2026-05-06T21:26:08Z
edges:
- target: comp-jam-svc-repo
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`pr-status(pr-ref)` → typed PR state (§5.4). Uses ETag-conditional GitHub request.

Implementation note (2026-05-06): MVP implementation is `tool.repo.pr-status` in `jam-svc-repo` using `gh pr view --json number,url,state,title,headRefName,isDraft`. It accepts `owner/repo#number`, GitHub PR URL, or a `gh pr view` selector plus optional `repo`.
