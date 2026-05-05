---
id: api-request-human-merge
type: api_surface
status: draft
created: 2026-05-04T03:52:34.050396440Z
updated: 2026-05-04T04:56:05.701610416Z
edges:
- target: comp-jam-svc-repo
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`request-human-merge(pr-ref, summary)` → notifies human via ntfy and UI (§5.4). **Only path to merge** — there is no `merge-pr` tool.

Canonical example of `principle-structure-in-tools-not-policy` and `insight-no-tool-no-possibility`.