---
id: task-opencode-deepseek-adapter-impl
type: task
status: blocked
created: 2026-05-04T03:59:07.840704027Z
updated: 2026-05-06T20:37:56Z
edges:
- target: feat-picker-layer-three-tier
  type: child_of
---
Phase 3 (§12). `OpenCodeAdapter` implementation of `HarnessAdapter`. Wraps OpenCode invocation with `tempyr journal bootstrap` prefix and `tempyr journal finalize` cleanup.

Per `comp-opencode-deepseek-adapter`.

Acceptance: spawn an OpenCode Picker with DeepSeek V4 Pro; verify Tempyr session opened+closed even on `full-stop` mid-task.

Implementation note (2026-05-06): `jam-svc-session` now treats `opencode-deepseek` as a live harness. The adapter verifies the OpenCode lockfile pin, generates a worktree-local `.jam/opencode-runner.sh`, `.jam/opencode-prompt.txt`, and `.jam/opencode.json`, injects only `DEEPSEEK_API_KEY` from `jam/pickers/deepseek-api-key` (or `JAM_SECRETS_FILE` / env override), configures DeepSeek V4 Pro / V4 Flash through OpenCode's current provider config shape, and wraps `opencode run --dir <worktree> --format json --dangerously-skip-permissions --model deepseek/deepseek-v4-pro`. The runner bootstraps Tempyr, writes a start journal entry for `agent=opencode`, tees JSON output to `.jam/opencode-events.jsonl`, and finalizes on exit; `full-stop` still performs supervisor-side `tempyr journal finalize --agent opencode --quiet` as fallback.

Smoke note (2026-05-06): local OpenCode 1.14.39 was installed for `caleb`, `maestro`, and `picker`; with the generated DeepSeek provider config, `opencode models deepseek --refresh` listed `deepseek/deepseek-v4-pro` and `deepseek/deepseek-v4-flash`. A temporary NATS smoke with a fake pinned OpenCode binary spawned `opencode-deepseek:*` sessions through `tool.session.spawn-picker`; the normal session exited 0 and produced a Tempyr `.ready` marker, and the `full-stop` session wrote `.killed-at-20260506T121431Z`, returned `tempyr_finalized: true`, and also produced a Tempyr `.ready` marker.

Usage smoke (2026-05-06): a temporary NATS smoke with a fake pinned OpenCode binary verified the generated wrapper's JSON tee and `journal.quota.usage-observed` publication. `jam-svc-observe` read the journaled usage and folded the fake `$0.42` cost into the configured API-budget state.

Credential note (2026-05-06): local runtime now has the canonical
`jam/pickers/deepseek-api-key` in maestro pass. It was copied from the older
`jam/workers/deepseek-api-key` path without printing the secret, so the
implemented adapter can resolve `DEEPSEEK_API_KEY` through the normal pass
backend.

Blocked note (2026-05-06): remaining acceptance is a real paid
`opencode-deepseek` Picker run through `tool.session.spawn-picker`, including
normal exit and full-stop Tempyr closure. I did not run it unattended because it
may consume DeepSeek API quota.
