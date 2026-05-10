---
id: api-prepare-merge
type: api_surface
status: stable
created: 2026-05-04T03:52:31.793669827Z
updated: 2026-05-06T22:38:00Z
edges:
- target: comp-jam-svc-repo
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`prepare-merge(pr_ref, repo?)` → final pre-merge checks, **doesn't merge** (§5.4). Implemented by `jam-svc-repo` as `tool.repo.prepare-merge`.

The service uses `gh pr view` and `gh pr checks` to return PR state, draft status, merge-state status, review decision, check rows, `checks_passed`, and conservative `ready`. Per `principle-no-auto-merge` and `dec-no-auto-merge-no-merge-pr-tool`.
