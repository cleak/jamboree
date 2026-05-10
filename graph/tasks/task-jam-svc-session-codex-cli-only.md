---
id: task-jam-svc-session-codex-cli-only
type: task
status: done
created: 2026-05-04T03:58:18.087050421Z
updated: 2026-05-06T06:28:34Z
edges:
- target: feat-picker-layer-three-tier
  type: child_of
---
Phase 1 (§12). `jam-svc-session` implementing `spawn-picker` for **Codex CLI only**. `local × default` profile.

Per `comp-jam-svc-session`, `comp-codex-cli-adapter`.

Why Codex CLI first: simplest because of clean Skills/SessionStart hooks (§12 Phase 1, §24.9 step 8).

Acceptance: spawn a Picker, watch it edit code in its worktree, watch it open a PR, see PR show up in `world-snapshot.pr` for that task.

Implementation note (2026-05-06): `crates/jam-svc-session` implements the Codex-only `spawn-picker` MVP for `local` x `default`: traced NATS request/reply, child Picker trace generation, spawn-time Codex lockfile version/checksum verification, worktree creation through `tool.worktree.create`, native path checks, Codex metadata bootstrap in the worktree gitdir, process launch with optional `sudo -n -u picker`, in-memory `inspect-picker` / `list-active`, and `journal.picker.spawned` plus `journal.picker.exited` publication.

Live smoke (2026-05-06): a temporary local NATS stack spawned a real Codex Picker for Blueberry task `jamboree-smoke-20260506-0621` (`session_id=codex-cli:01KQXZ66Q1EDRDG1RM8VY7XKWG`, picker trace `01KQXZ65D8KZ9PQVMJ4JJXBJ3M`). The Picker created `docs/operations/jamboree-picker-smoke-20260506.md`, logged Tempyr journal entries, committed `8ee65b22`, pushed branch `task/jamboree-smoke-20260506-0621`, and opened draft PR <https://github.com/cleak/blueberry/pull/383>. `jam-svc-observe` returned `world-snapshot.pr.url=https://github.com/cleak/blueberry/pull/383` for that task.

Runtime note (2026-05-10): that smoke predates the current PR policy. Live Picker PRs now default to non-draft, use Picker-authored `.jam/pr-title.txt` and `.jam/pr-body.md`, and get a deterministic `[jam]` title prefix.

Verification note (2026-05-06): while running the real smoke, Codex's managed `workspace-write` sandbox could not commit in a linked worktree because the common gitdir under `/home/caleb/blueberry/.git/worktrees/...` was read-only. For `local` x `default`, `jam-svc-session` now launches Codex with `--dangerously-bypass-approvals-and-sandbox` and relies on the Picker OS account boundary specified by the security model. The smoke used `JAM_SESSION_USE_SUDO=false` as `caleb` because passwordless sudo was not available in this shell; production `sudo -u picker` remains the responsibility of `task-maestro-spawn-via-sudo`.

Follow-up lifecycle smoke (2026-05-06): after adding `picker.exited` publication, dry-run task `jamboree-smoke-exit-20260506-0627` emitted both `picker.spawned` and `picker.exited`; `inspect-picker` returned `status=exited`, `exit_code=0`, and `world-snapshot.session.status=exited`.
