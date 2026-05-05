---
id: api-prepare-merge
type: api_surface
status: draft
created: 2026-05-04T03:52:31.793669827Z
updated: 2026-05-04T04:55:56.470026832Z
edges:
- target: comp-jam-svc-repo
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`prepare-merge(pr-ref)` → final pre-merge checks, **doesn't merge** (§5.4). Per `principle-no-auto-merge` and `dec-no-auto-merge-no-merge-pr-tool`.