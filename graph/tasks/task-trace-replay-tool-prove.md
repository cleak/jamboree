---
id: task-trace-replay-tool-prove
type: task
status: done
created: 2026-05-04T03:58:43.978091789Z
updated: 2026-05-06T06:45:28Z
edges:
- target: feat-trace-propagation
  type: child_of
---
Phase 1 (§12). `trace-replay` tool — proves the trace chain works end-to-end.

Per `comp-trace-replay-tool`, `api-trace-replay`.

Acceptance: trace from Picker spawn back to Maestro wake reconstructible via `trace-replay`. Kill the Picker mid-session (`full-stop`); verify worktree is preserved with `.killed-at-` marker, Tempyr task node updated to `abandoned`, journal session finalized cleanly.

Implementation note (2026-05-06): `crates/jam-cli` now implements the first durable-journal trace replay surface at `jam trace replay <trace-id>`. It validates ULID trace IDs, reads `JAM_HOME/journal/**/journal.*.jsonl`, walks `parent_trace_id` links up to `--max-depth`, and prints a chronological chain with event type, actor, source file/line, task/session/worktree context, and parent trace. Unit coverage verifies parent-chain traversal and invalid trace rejection. Live smoke against `/tmp/jam-real-picker-20260506T060820Z/jam-home` replayed real Codex Picker trace `01KQXZ65D8KZ9PQVMJ4JJXBJ3M` and reconstructed parent trace `01K90CFPWEWXBHHHH76NYP9309` from `worktree.created` and `picker.spawned` journal entries.

Full-stop note (2026-05-06): `jam-svc-session` now has the Phase 1 adapter-level `tool.session.full-stop` path for the Codex/local/default MVP. It launches Pickers in their own Unix process group, sends `SIGTERM` with a configurable grace window then `SIGKILL`, writes `.killed-at-<utc>` in the preserved worktree, records a git status/diff snapshot, attempts `tempyr journal log --agent codex outcome ... --final`, publishes `journal.picker.killed`, and publishes `journal.task.abandoned` so `jam-task-lifecycle` marks the task abandoned. The final supervisor/message-service split remains tracked by `comp-jam-svc-message` and §5.7.

Live full-stop smoke (2026-05-06): temporary NATS root `/tmp/jam-full-stop-20260506T064153Z` spawned dry-run Picker task `jamboree-full-stop-smoke-20260506-0644` (`session_id=codex-cli:01KQY0G82C6AFBEBYWK8QH5GQ7`, picker trace `01KQY0G6S81FBARSQTZ2X095HP`), then called `tool.session.full-stop` with reason `full-stop smoke test after kill fix`. Verification: process pid `45604` no longer existed, marker `/tmp/jam-full-stop-20260506T064153Z/worktrees/jamboree-full-stop-smoke-20260506-0644/.killed-at-20260506T064455Z` existed, `journal.picker.jsonl` contained `picker.killed`, `journal.task.jsonl` contained `task.abandoned`, lifecycle node `/tmp/jam-full-stop-20260506T064153Z/canonical/graph/tasks/jamboree-full-stop-smoke-20260506-0644.md` had `status: abandoned`, `tempyr journal lint` passed in the preserved worktree, and `jam trace replay 01KQY0G6S81FBARSQTZ2X095HP` reconstructed the child trace plus parent `01K1M7K4JD602SFC2EVVH27MF7`.
