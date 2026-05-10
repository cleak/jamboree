---
id: feat-maestro-tool-surface
type: feature
status: active
created: 2026-05-04T03:28:20.409793378Z
updated: 2026-05-04T05:06:20.143235226Z
owner: caleb
edges:
- target: api-archive-session
  type: exposes
- target: api-branch-staleness
  type: exposes
- target: api-classify-review-artifacts
  type: exposes
- target: api-compute-readiness
  type: exposes
- target: api-enqueue-message
  type: exposes
- target: api-find-conflicts
  type: exposes
- target: api-find-traces
  type: exposes
- target: api-full-stop
  type: exposes
- target: api-inspect-picker
  type: exposes
- target: api-interrupt-with-message
  type: exposes
- target: api-list-active-pickers
  type: exposes
- target: api-list-blockers
  type: exposes
- target: api-list-review-artifacts
  type: exposes
- target: api-mark-review-artifact-handled
  type: exposes
- target: api-mcp-discover-and-load
  type: exposes
- target: api-notify-human
  type: exposes
- target: api-open-pr
  type: exposes
- target: api-pause-dispatch
  type: exposes
- target: api-pr-status
  type: exposes
- target: api-prepare-merge
  type: exposes
- target: api-propose-tool-change
  type: exposes
- target: api-purge-session
  type: exposes
- target: api-query-quota
  type: exposes
- target: api-query-session-store
  type: exposes
- target: api-query-tempyr
  type: exposes
- target: api-read-journal
  type: exposes
- target: api-read-pr-comments
  type: exposes
- target: api-read-skills
  type: exposes
- target: api-record-improvement-candidate
  type: exposes
- target: api-record-learning
  type: exposes
- target: api-record-tempyr-update-candidate
  type: exposes
- target: api-refresh-world-snapshot
  type: exposes
- target: api-reply-to-comment
  type: exposes
- target: api-request-human-merge
  type: exposes
- target: api-request-research
  type: exposes
- target: api-request-review
  type: exposes
- target: api-request-skill-evolution
  type: exposes
- target: api-spawn-picker
  type: exposes
- target: api-tempyr-journal-blame
  type: exposes
- target: api-tempyr-journal-range
  type: exposes
- target: api-tempyr-journal-search
  type: exposes
- target: api-trace-replay
  type: exposes
- target: api-web-crawl
  type: exposes
- target: api-web-extract
  type: exposes
- target: api-web-search
  type: exposes
- target: api-worktree-diff
  type: exposes
- target: api-world-snapshot
  type: exposes
- target: api-world-snapshot-delta
  type: exposes
- target: comp-jam-svc-evolve
  type: uses
- target: comp-jam-svc-knowledge
  type: uses
- target: comp-jam-svc-message
  type: uses
- target: comp-jam-svc-observe
  type: uses
- target: comp-jam-svc-repo
  type: uses
- target: comp-jam-svc-research
  type: uses
- target: comp-jam-svc-search
  type: uses
- target: comp-jam-svc-session
  type: uses
- target: comp-jam-svc-supervise
  type: uses
- target: comp-jam-svc-worktree
  type: uses
- target: comp-jam-trace-crate
  type: uses
- target: dec-no-auto-merge-no-merge-pr-tool
  type: depends_on
- target: dec-no-auto-rebase
  type: depends_on
- target: insight-no-tool-no-possibility
  type: informed_by
- target: jamboree-v5
  type: child_of
- target: principle-no-auto-merge
  type: constrained_by
- target: principle-no-auto-rebase
  type: constrained_by
- target: principle-structure-in-tools-not-policy
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
- target: principle-untrusted-content-cannot-issue-commands
  type: constrained_by
- target: task-tool-surface-pr-comments
  type: parent_of
---
The complete Maestro tool API (§5). All tools use kebab-case names; inputs validated by Pydantic on the Maestro side and Rust types on the tool service side, with JSON schema as the contract (§11.2.6). Every tool call carries a `trace_id` (§23).

Surface buckets:
- §5.1 Observation
- §5.2 Session lifecycle
- §5.3 Worktree management
- §5.4 Repo / PR ops
- §5.5 Knowledge / context
- §5.6 Search / research
- §5.7 Messaging (queue / interrupt / full-stop)
- §5.8 Trace and meta tools
- §5.9 Deliberately absent (no `merge-pr`, `read-file`/`write-file`/`run-command`, `eval`/`exec`, `auto-rebase`, etc.)

The deliberately-absent category is the primary enforcement of §2.3 — invariants live in tool absence.

Implementation note (2026-05-06): the Python Maestro scaffold now has `MaestroToolRegistry`, a callable-tool allowlist for the current generated tool request models. It explicitly rejects the §5.9 absent names at registry construction and returns stable `no such tool` errors for absent or unknown calls.

Implementation note (2026-05-06): `notify-human` is now an allowlisted typed route backed by `crates/jam-tools-core/schemas/supervise/notify-human.request.json` and `tool.supervise.notify-human`.
