---
id: dec-github-app-not-pat
type: decision
status: decided
created: 2026-05-04T03:46:19.746629282Z
updated: 2026-05-11T07:30:00Z
edges:
- target: comp-github-app-client
  type: decision_for
- target: feat-reviewer-adapters
  type: depended_on_by
---
**GitHub App authentication, not PAT** (§4.7.1).

Reasons:
- 3x rate limit ceiling (15K/hour vs 5K).
- Per-installation rate limits — a noisy reviewer adapter doesn't starve other components.
- ETag-conditional requests count against the limit only for non-304 responses.

Setup is one-time: register the app, generate private key, install on repos, store app id / installation id / key in `pass` (`jam/pickers/github-app-id`, `jam/pickers/github-app-installation-id`, `jam/pickers/github-app-key`), exchange for installation tokens at startup. The `octocrab` crate handles the dance.

Picker secrets distribution: harness adapter exchanges App key → installation token → picker-scoped token before spawn. Token expires in 1 hour; refresh logic in adapter reissues for long-running Pickers via NATS callback.

With ETag caching, ~70% of polls return 304 — plenty for 30s polling on 10+ active PRs.

## Addendum 2026-05-11 — user-to-server tokens for PR writes

Installation tokens (server-to-server) attribute every action to `app/<name>` with `is_bot:true`. Reviewer bots that filter bot-authored PRs — CodeRabbit is the immediate example, and it's load-bearing in our review pipeline — hard-skip those PRs. No CodeRabbit config opts back into review; the comment-trigger workaround (`@coderabbitai full review`) only fires once and gets stuck behind incremental-skip on subsequent pushes.

**Resolution.** Keep the App for read-heavy paths (the 15K/hour ceiling matters for polling); add a **user-to-server token** authorized by a human user (`cleak`) and use it for the *write* path: `gh pr create`, `git push`, and PR-comment writes from `jam-svc-repo`. PRs open as the authorizing user with `is_bot:false`, so reviewer bots auto-review through the normal path.

Caveats this clears up:
- User-to-server tokens get the 5K/hour limit — *not* the App's 15K. The rate-limit reason for picking the App above applies only to installation tokens used for reads (poller, status checks, ETag-conditional GETs). PR-creation is one write per task, so the limit doesn't bite.
- Configure the App's "Expire user authorization tokens" Optional Feature = OFF so the `ghu_*` token is non-expiring. Otherwise we'd need refresh-token plumbing.

Operational setup:
- One-time device flow via `scripts/authorize-github-user-token.sh`, run as `caleb`, populates `jam/pickers/github-user-token` in maestro's pass store.
- `jam-svc-repo` prefers the user token (`resolve_write_token`) over the installation token; falls back to installation token + the comment-trigger workaround when user-token is missing, so the system still functions during the seed window.
- This decision does **not** authorize PATs for any other purpose.
