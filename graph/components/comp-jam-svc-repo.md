---
id: comp-jam-svc-repo
type: component
status: active
created: 2026-05-04T03:39:33.056690005Z
updated: 2026-05-10T00:00:00Z
edges:
- target: api-mark-review-artifact-handled
  type: exposes
- target: api-open-pr
  type: exposes
- target: api-pr-status
  type: exposes
- target: api-prepare-merge
  type: exposes
- target: api-read-pr-comments
  type: exposes
- target: api-reply-to-comment
  type: exposes
- target: api-request-human-merge
  type: exposes
- target: api-request-review
  type: exposes
- target: comp-events-toml-and-codegen
  type: depends_on
- target: comp-jam-trace-crate
  type: depends_on
- target: comp-nats-jetstream
  type: depends_on
- target: comp-routing-manifest
  type: depends_on
- target: feat-maestro-tool-surface
  type: used_by
- target: feat-tool-services-out-of-process
  type: used_by
- target: principle-decoupled-processes-bus
  type: constrained_by
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
---
Repo / PR ops tool service. Subject prefix `tool.repo.*`. Crate `crates/jam-svc-repo/`.

Tools (§5.4):
- `open-pr(branch, title, body, draft?)` → `PullRequestRef`
- `pr-status(pr-ref)` → typed PR state
- `read-pr-comments(pr-ref)` → `Vec<ReviewArtifact>`
- `reply-to-comment(artifact-id, text)` → posts reply via reviewer adapter
- `mark-review-artifact-handled(artifact-id, status, reasoning)`
- `request-review(pr-ref, reviewer-id)`
- `prepare-merge(pr-ref)` — final pre-merge checks, doesn't merge
- `request-human-merge(pr-ref, summary)` — notifies human via ntfy and UI; **only path to merge**

Reviewer adapter implementations and GitHub App client live here.

Implementation note (2026-05-06): `crates/jam-svc-repo` now exists with a narrow `gh`-backed MVP for `tool.repo.open-pr`, `tool.repo.pr-status`, and `tool.repo.ping`. `open-pr` can push a Picker branch from `worktree_path`, runs `gh pr create --head <branch>` without prompts, and emits `journal.pr.opened`.

Review-tool note (2026-05-06): `read-pr-comments`, `reply-to-comment`, and `mark-review-artifact-handled` are operational on the same `gh` fallback backend. `read-pr-comments` fetches GitHub issue comments, PR review comments, and PR reviews through `gh api`, wraps outside-authored bodies in `Untrusted<String>` before exposing JSON data, and emits stable artifact IDs such as `github-review-comment:owner/repo#42:123`. `reply-to-comment` posts threaded replies for review comments and top-level PR comments for issue/review artifacts. `mark-review-artifact-handled` appends trusted local state to `JAM_REVIEW_ARTIFACT_STATE_PATH` / `$JAM_HOME/review-artifacts-handled.jsonl` and publishes `journal.review-artifact.handled`. This is still not the final GitHub App backend from §4.7.1; it preserves the tool shape while Phase 2 auth work remains blocked on App credentials.

GitHub App note (2026-05-06): the service can now optionally authenticate repo operations with a GitHub App installation token. If App ID, installation ID, and private key are configured through env, `JAM_SECRETS_FILE`, or maestro pass, it uses Octocrab's App flow to exchange for an installation token, passes it to `gh` as `GH_TOKEN`, and passes it to `git push` through a constant credential helper with terminal prompts disabled. Mock tests cover the token exchange and push credential path; real App registration remains external.

Merge-escalation note (2026-05-06): `prepare-merge` is now a read-only `gh pr view`/`gh pr checks` preflight that returns conservative readiness without mutating the PR. `request-human-merge` wraps that preflight and calls `tool.supervise.notify-human` through NATS; it remains an escalation path only and does not merge.

Runtime note (2026-05-10): `open-pr` defaults to `draft=false`, applies the `[jam]` title prefix deterministically, and rejects low-information titles. This keeps CodeRabbit eligible to review new Picker PRs and prevents task IDs or log lines from becoming reviewer-facing PR titles.
