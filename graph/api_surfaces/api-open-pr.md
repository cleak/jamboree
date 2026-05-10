---
id: api-open-pr
type: api_surface
status: stable
created: 2026-05-04T03:52:18.414449667Z
updated: 2026-05-10T00:00:00Z
edges:
- target: comp-jam-svc-repo
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`open-pr(branch, title, body, draft?)` → `PullRequestRef` (§5.4). Wraps GitHub App-authenticated `octocrab` call.

Implementation note (2026-05-06): MVP implementation is `tool.repo.open-pr` in `jam-svc-repo` using `gh pr create` as a temporary backend. Inputs include `task_id`, `branch`, `title`, optional `body`, `draft`, `base`, `repo`, `worktree_path`, and `push`; output includes `task_id`, `pr_ref`, `url`, `branch`, `title`, `draft`, `state`, `opened_at`, and `trace_id`.

Runtime note (2026-05-10): PR creation defaults to non-draft. The repo service deterministically normalizes titles to start with `[jam]` and rejects titles that are empty, ID-like, branch-like, or raw log text. Picker-created PR metadata should come from `.jam/pr-title.txt` and `.jam/pr-body.md`, written before successful Picker exit.
