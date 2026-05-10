---
id: comp-jam-svc-session
type: component
status: active
created: 2026-05-04T03:39:30.775848907Z
updated: 2026-05-10T00:00:00Z
edges:
- target: api-archive-session
  type: exposes
- target: api-inspect-picker
  type: exposes
- target: api-list-active-pickers
  type: exposes
- target: api-purge-session
  type: exposes
- target: api-spawn-picker
  type: exposes
- target: comp-events-toml-and-codegen
  type: depends_on
- target: comp-github-app-client
  type: depends_on
- target: comp-harness-adapter-trait
  type: depends_on
- target: comp-harness-version-lockfile
  type: depends_on
- target: comp-jam-secrets
  type: depends_on
- target: comp-jam-trace-crate
  type: depends_on
- target: comp-nats-jetstream
  type: depends_on
- target: comp-routing-manifest
  type: depends_on
- target: comp-sandbox-backend-trait
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
The session tool service. Subject prefix `tool.session.*`. Crate `crates/jam-svc-session/`. Owns `spawn-picker` (§24.3) and the harness adapter implementations.

Tools: `spawn-picker`, `inspect-picker`, `list-active`, `archive-session`, `purge-session` (§5.2).

Spawn protocol (§24.3):
1. Generate child trace.
2. Verify quota.
3. Worktree creation via NATS request to jam-svc-worktree.
4. Verify harness lockfile according to `JAM_HARNESS_LOCKFILE_POLICY`.
5. Sandbox prep via SandboxBackend.
6. Path safety invariants (§6.6).
7. Bootstrap Tempyr journal session for the Picker.
8. Get short-lived GitHub installation token.
9. Get harness-specific secrets via per-harness allowlist.
10. Launch (under multi-user model, via `sudo -n -u picker --preserve-env=...` per security-setup §7.2).
11. Emit `journal.picker.spawned` event with trace_ids in payload.

Implementation note (2026-05-06): the Codex-only launch path is implemented with optional sudo wrapping. In sudo mode, `jam-svc-session` clears the wrapper environment, applies the Picker allowlist, derives `--preserve-env` from that same allowlist, starts the wrapper from `/`, and passes Codex `--cd <picker-worktree>` so the worktree chdir happens after the uid switch to `picker`.

Implementation note (2026-05-06): `spawn-picker` now supports live `codex-cli`, `claude-code`, and `opencode-deepseek` launches. The Claude path writes merged `.claude/settings.json` Tempyr hooks, generates a Claude MCP config from the Blueberry project config, verifies the `claude-code` lockfile pin, and launches the current CLI with `--mcp-config` plus `--strict-mcp-config`. Direct Codex and Claude launches capture JSON stdout in worktree-local `.jam/codex-events.jsonl` and `.jam/claude-events.jsonl` logs for quota usage parsing; fake pinned Codex NATS smoke proved stdout capture through spawn, exit, `journal.quota.usage-observed`, and observe query. Real Codex/Claude one-word schema samples are covered: Codex usage arrives on `turn.completed`, while Claude result summaries are preferred over assistant usage events to avoid double counting. The OpenCode path writes a generated runner/config/prompt under `.jam/`, injects only the DeepSeek API key from the Jamboree secrets path, verifies the `opencode-deepseek` lockfile pin, finalizes Tempyr sessions through the wrapper plus full-stop fallback, and parses the OpenCode JSON event log after exit to publish `journal.quota.usage-observed`.

Implementation note (2026-05-06): `spawn-picker` now has a Docker sandbox backend in addition to local/sudo launch. Docker spawns reuse the same harness argument construction but wrap it in `docker run` with `/work` read-write, `/repo.git` read-only, read-only rootfs, tmpfs for `/tmp` and HOME, explicit profile network policy, and the same traced Picker env allowlist. Picker env now includes `JAM_SANDBOX_BACKEND` and `JAM_SANDBOX_PROFILE` so harness wrappers and smokes can confirm the effective sandbox.

Implementation note (2026-05-06): local spawns now use cgroup v2 resource scopes through `systemd-run --user --scope` by default. The wrapper preserves the existing env-clearing behavior for Pickers while allowing the systemd user bus variables needed by `systemd-run`, applies task-class CPU/memory/I/O properties, and reports the transient systemd scope in `resource_scope`.

Lifecycle note (2026-05-06): `archive-session` and `purge-session` are implemented for completed sessions. Archive removes the session from service state and journals `journal.session.archived` while retaining the worktree/artifacts. Purge requires a reason, refuses running/killing sessions, publishes `journal.task.abandoned` plus `journal.session.purged`, and deletes the retained worktree unless `preserve_worktree=true`.

Runtime note (2026-05-10): the live Blueberry deployment sets `JAM_HARNESS_LOCKFILE_POLICY=warn`, `JAM_SESSION_OPEN_PR_DRAFT=false`, and captures Picker output under `/home/maestro/.jam/session-logs/<session_id>.jsonl`. The service asks Pickers to write `.jam/pr-title.txt` and `.jam/pr-body.md`; Jamboree adds the `[jam]` PR title prefix deterministically.
