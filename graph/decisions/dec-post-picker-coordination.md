---
id: dec-post-picker-coordination
type: decision
status: decided
created: 2026-05-12T03:50:00Z
updated: 2026-05-12T03:50:00Z
edges:
- target: comp-jam-task-lifecycle
  type: decision_for
- target: comp-jam-svc-session
  type: decision_for
---
**The orchestrator opens PRs and routes review feedback, not the Picker.**

## Why this exists

Pickers were previously expected to call `gh pr create` or `tool.repo.open-pr` as their final step. They unreliably did. The 2026-05-12 incident (task `pick-a-small-task-...-vmqkqe`): Picker exited cleanly with one commit and `.jam/pr-title.txt` + `.jam/pr-body.md` written, but no PR was opened. Nothing in the system consumed that prepared metadata. Caleb noticed an hour later.

CodeRabbit review comments and CI failures had the same shape: they arrived on the PR with no one watching, and reverting to manual triage every time defeats the orchestrator. The user constraint that closed this off:

> The pr-daemon shouldn't rely on the picker having done something specific (other than write the code and commit it). If state is wrong, kick work back to the picker. Also, monitor PR for comments/CI and return to a picker with equal context — don't address comments in a vacuum.

## Decision

`jam-task-lifecycle` becomes the post-picker coordinator, subscribing to `picker.exited`, `picker.continuation-needed`, `pr.review-received`, and `pr.ci.status-changed`. It owns:

1. **PR open** on clean Picker exit (the `.jam/pr-*` metadata contract).
2. **Continuation spawn** whenever state isn't right (worktree dirty, no commits, missing metadata, CI failed, comments arrived).

The Picker's contract becomes deliberately small (cite §2.1 — *more observable, not more deterministic*):
- Make the code change.
- Commit it on the task branch.
- Write `.jam/pr-title.txt` (one line, conventional prefix) and `.jam/pr-body.md` (Summary + Verification).
- Leave the working tree clean.
- Exit.

The Picker MUST NOT push or call `gh pr create` itself. The coordinator does both, using maestro's user-to-server GitHub token so PRs are attributed to a real user (`is_bot:false`) — see `dec-github-app-not-pat`.

## Pre-checks (on picker.exited, exit_code=0)

Run as `maestro` via `sudo -u picker`:

1. Worktree directory exists and is readable.
2. `git rev-list --count origin/<base>..HEAD` ≥ 1.
3. `git status --porcelain` is empty after filtering `.jam/` paths.
4. `.jam/pr-title.txt` exists, non-empty (≤240 chars).
5. `.jam/pr-body.md` exists, non-empty.

All pass → publish a `tool.repo.open-pr` request via NATS request-reply. Any fail → publish `picker.continuation-needed` with a specific `reason` enum value.

## Continuation reasons

| Reason | Trigger | Resume prompt content |
|---|---|---|
| `picker-failed` | `picker.exited` with exit_code ≠ 0 | "Diagnose why the previous session exited non-zero, fix it, commit, write `.jam/pr-*`." |
| `no-commits` | Pre-check 2 failed | "Make the code changes the task requires, commit them, write `.jam/pr-*`." |
| `dirty-tree` | Pre-check 3 failed | "Commit or revert the uncommitted changes (listed). Then exit." |
| `missing-pr-metadata` | Pre-check 4 or 5 failed | "Write `.jam/pr-title.txt` (conventional title) and `.jam/pr-body.md` (Summary + Verification)." |
| `open-pr-failed` | NATS `tool.repo.open-pr` returned an error envelope | "Inspect the open-pr error and correct the cause." |
| `review-received` | `pr.review-received` for an open PR | "Read the new review comments via `read-pr-comments`. Address each in the worktree; commit on the existing branch (no amend/force-push)." |
| `ci-failed` | `pr.ci.status-changed` with status in {failure, error, cancelled, timed_out, action_required} | "Inspect the failing CI logs via `gh pr checks` / `gh run view --log-failed`, reproduce locally, fix, commit." |

## Continuation spawn primitive

`tool.session.resume-picker` (new method) takes `{task_id, prompt, parent_session_id?, task_class?}` and runs `codex exec resume --last --cd <worktree> <prompt>`. The `--last` flag is cwd-filtered by codex, and worktrees are 1:1 with tasks, so the most-recent prior session in that worktree is unambiguously the right one. No need to capture or journal codex's UUID separately. Reuses the same `launch_picker` machinery (sudo wrapping, sandbox profile, output watching, journal events) as `spawn-picker`; only `create_worktree` and harness-lockfile re-verification are skipped because the worktree already exists.

The new `picker.spawned` v2 schema adds optional `parent_session_id` (chain-of-continuation), so the resumed session links back to the original — this is what the coordinator uses to count `attempt` and gate against runaway loops.

## Loop-guard

`CONTINUATION_ATTEMPT_CAP = 3`. On the 4th continuation for a task, the coordinator logs and skips. The journal entries are still there for `jam trace replay` / human review.

(Counting `attempt` is currently approximate — the coordinator increments from the originating `picker.continuation-needed` envelope's `attempt` field. A more rigorous count via journal scan is a follow-up.)

## Known limitation: feedback-loop push

When a continuation is triggered by `review-received` or `ci-failed`, the Picker makes additional commits on the **existing** PR branch. After the continuation exits cleanly, the post-picker coordinator's `tool.repo.open-pr` call will fail (the PR already exists) and emit `open-pr-failed` → another continuation. Two fixes are possible:

1. `tool.repo.open-pr` becomes idempotent: if a PR exists for the branch, just push the new commits and return the existing pr_ref. **Preferred.**
2. The coordinator detects an existing PR via journal lookup and skips the open-pr call (push-only).

Tracked as a follow-up; v1 ships with the loop-guard absorbing the second continuation cleanly.

## Principles cited

- §2.1 — more observable, not more deterministic. The signal is the state of the worktree + journal events, not a Picker self-declaration.
- §2.12 — failure surfaces immediately. Each pre-check failure publishes a specific `reason` event; no silent degradation.
- §2.13 — tracing chains. Continuation events carry the trace forward; resumed Pickers inherit via the standard `parent_trace_id` mechanism.

## Affected components

- `crates/jam-events/events.toml` — new `picker.continuation-needed` event; `picker.spawned` bumped to v2 with optional `codex_conversation_id` (reserved) + `parent_session_id`.
- `crates/jam-task-lifecycle/src/post_picker.rs` — new module: pre-checks, open-pr request, continuation publish, PR-feedback handlers, continuation consumer.
- `crates/jam-svc-session/src/main.rs` — new `resume-picker` tool method; `SpawnSpec.resume_from_last` flag; shared `launch_picker` body extracted from `spawn_picker`.
- `skills/projects/blueberry/commit-validation.md` — Picker contract narrowed: write `.jam/pr-*`, don't push, don't open PR.
