---
id: comp-codex-review-adapter
type: component
status: active
created: 2026-05-04T03:34:48.504277139Z
updated: 2026-05-04T04:43:06.649019085Z
edges:
- target: comp-github-app-client
  type: depends_on
- target: comp-reviewer-adapter-trait
  type: depends_on
- target: feat-reviewer-adapters
  type: used_by
---
codex-review reviewer adapter. Phase 2 add (§12.2).

Implementation note (2026-05-06): the installed OpenAI Codex CLI exposes `codex review` rather than a standalone `codex-review` binary. `jam-svc-repo` now implements `tool.repo.request-review` for `reviewer_id="codex-review"` by running `codex -C <worktree> review --base <base>`, normalizing non-empty stdout into a `codex-review:<pr-ref>:<trace-suffix>` review artifact with `body_trust: untrusted`, and publishing `journal.pr.review-received`. Unit coverage uses a fake Codex binary, so no review credits are spent in local tests. Live acceptance still requires a real PR/worktree run plus GitHub App-backed read/reply handling.
