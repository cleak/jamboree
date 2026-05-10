---
id: task-codex-review-reviewer-adapter
type: task
status: blocked
created: 2026-05-04T03:58:54.877620354Z
updated: 2026-05-06T20:37:56Z
edges:
- target: feat-reviewer-adapters
  type: child_of
---
Phase 2 (§12). codex-review reviewer adapter implementing `ReviewerAdapter` trait.

Per `comp-codex-review-adapter`.

Implementation note (2026-05-06): local discovery found the installed Codex CLI provides `codex review`; there is no separate `codex-review` binary. `jam-svc-repo` now implements `request-review` for `reviewer_id="codex-review"` by requiring a native `worktree_path`, running `codex -C <worktree> review --base <base>`, normalizing stdout into an untrusted review artifact, and publishing `journal.pr.review-received`. Maestro has a typed `RepoRequestReviewRequest` route for the tool. Unit tests use a fake Codex binary to verify command shape and avoid spending review credits.

Blocked note (2026-05-06): full acceptance still needs a real PR/worktree `codex review` run and the shared GitHub App client for production read/reply tokens. The local maestro pass store has the GitHub App ID/key, but the App installation ID is still missing and `/app/installations` returned zero installations. To finish acceptance, install the App on Blueberry, seed `jam/pickers/github-app-installation-id`, run `request-review` on a real PR worktree, then fetch/reply/mark handled through installation-token auth.
