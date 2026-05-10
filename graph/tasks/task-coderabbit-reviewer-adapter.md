---
id: task-coderabbit-reviewer-adapter
type: task
status: blocked
created: 2026-05-04T03:58:52.133689003Z
updated: 2026-05-06T20:37:56Z
edges:
- target: feat-reviewer-adapters
  type: child_of
---
Phase 2 (§12). CodeRabbit reviewer adapter implementing `ReviewerAdapter` trait.

Per `comp-coderabbit-adapter`, `comp-reviewer-adapter-trait`.

Acceptance: PR with CodeRabbit comments: Maestro reads them, classifies them, decides which to address, dispatches a Picker with the reasoning, marks them handled.

Local repo-tool note (2026-05-06): `jam-svc-repo` now implements the generic PR comment tool surface through the current `gh api` fallback: `read-pr-comments`, `reply-to-comment`, and `mark-review-artifact-handled`. Mocked `gh` tests cover normalizing outside-authored comments into untrusted artifacts, threaded reply posting for PR review comments, and local handled-state JSONL writes. This gives the Maestro a working local read/reply/handled path independent of the future GitHub App backend.

Blocked note (2026-05-06): the synthetic prompt-injection/no-`merge-pr` path is already covered in Maestro tests, and the generic GitHub PR comment tool path now has mock coverage, but the adapter acceptance still needs a real PR with CodeRabbit comments plus GitHub App installation-token access. No CodeRabbit CLI/API surface is installed locally, and `task-github-app-registration` is blocked on the missing GitHub App installation ID. To finish acceptance, enable CodeRabbit on a test PR, install the GitHub App on Blueberry, seed the installation ID, then verify read/classify/reply/handled flow through installation-token auth.
