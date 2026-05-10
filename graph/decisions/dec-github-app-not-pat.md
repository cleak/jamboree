---
id: dec-github-app-not-pat
type: decision
status: decided
created: 2026-05-04T03:46:19.746629282Z
updated: 2026-05-04T05:02:26.596192084Z
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
