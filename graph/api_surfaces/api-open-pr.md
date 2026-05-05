---
id: api-open-pr
type: api_surface
status: draft
created: 2026-05-04T03:52:18.414449667Z
updated: 2026-05-04T04:55:03.582348138Z
edges:
- target: comp-jam-svc-repo
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`open-pr(branch, title, body, draft?)` → `PullRequestRef` (§5.4). Wraps GitHub App-authenticated `octocrab` call.