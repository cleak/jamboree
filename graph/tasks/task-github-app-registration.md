---
id: task-github-app-registration
type: task
status: backlog
created: 2026-05-04T03:58:46.705875940Z
updated: 2026-05-04T04:11:03.014645146Z
edges:
- target: feat-reviewer-adapters
  type: child_of
---
Phase 2 (§12). GitHub App registration + installation token exchange via `octocrab`.

Per `comp-github-app-client`, `dec-github-app-not-pat`, `dec-etag-conditional-requests`.

Acceptance: `octocrab` exchanges App private key for installation token; token used for `git push` and PR comment APIs.