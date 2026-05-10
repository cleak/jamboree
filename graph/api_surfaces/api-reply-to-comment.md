---
id: api-reply-to-comment
type: api_surface
status: stable
created: 2026-05-04T03:52:25.067329866Z
updated: 2026-05-06T21:26:08Z
edges:
- target: comp-jam-svc-repo
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`reply-to-comment(artifact-id, text)` → posts reply via reviewer adapter (§5.4). Capability-gated: only when the reviewer adapter's `supports_reply == true`.

The Maestro request contract is generated as `RepoReplyToCommentRequest` and
routed to `tool.repo.reply-to-comment`.

Implementation note (2026-05-06): `jam-svc-repo` now implements this on the `gh api` fallback backend. `github-review-comment:*` artifacts use GitHub's threaded PR-review-comment reply endpoint; issue-comment and review-summary artifacts post a normal PR/issue comment.
