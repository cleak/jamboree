---
id: task-trunk-fetcher-impl
type: task
status: done
created: 2026-05-04T04:01:36.767923851Z
updated: 2026-05-06T07:11:56Z
---
Implement `trunk-fetcher` reconciler — periodic `git fetch origin --prune` every 5min for each project's trunk.

Per `comp-trunk-fetcher`, §21.3.

Implementation note (2026-05-06): `crates/jam-trunk-fetcher` implements the watcher daemon. It replays active worktrees from `JAM_HOME/journal/**/journal.worktree.jsonl`, subscribes to `journal.worktree.created`, fetches the configured remote every 300s by default, emits `journal.branch.trunk-moved` when the configured trunk ref changes, and emits `journal.branch.staleness-updated` with behind/ahead counts from `git rev-list --left-right --count <trunk-ref>...HEAD`. It never rebases, merges, or edits Picker worktrees (`principle-no-auto-rebase`).

Live smoke (2026-05-06): temporary NATS root `/tmp/jam-trunk-fetcher-smoke-k11zae` used a local bare Git remote plus linked worktree. After pushing one new commit to remote `master`, `jam-trunk-fetcher --once` fetched `origin`, emitted `branch.trunk-moved` trace `01KQY1ZWHAGV42QSCSQX9D4RBE`, and emitted `branch.staleness-updated` for `trunk-fetcher-smoke-task` with `commits_behind=1`, `commits_ahead=0`. `jam-svc-observe` then returned `world_snapshot.branch_staleness.commits_behind=1` from the journal.
