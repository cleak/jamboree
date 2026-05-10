---
id: api-request-human-merge
type: api_surface
status: stable
created: 2026-05-04T03:52:34.050396440Z
updated: 2026-05-06T22:38:00Z
edges:
- target: comp-jam-svc-repo
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`request-human-merge(pr_ref, summary, repo?)` → runs `prepare-merge`, then notifies the Manager via `tool.supervise.notify-human` (§5.4). Implemented by `jam-svc-repo` as `tool.repo.request-human-merge`.

This is the only path to merge escalation. It still does not merge: there is no `merge-pr` tool. Canonical example of `principle-structure-in-tools-not-policy` and `insight-no-tool-no-possibility`.
