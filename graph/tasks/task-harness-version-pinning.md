---
id: task-harness-version-pinning
type: task
status: done
created: 2026-05-04T03:59:10.627537984Z
updated: 2026-05-10T00:00:00Z
edges:
- target: feat-picker-layer-three-tier
  type: child_of
---
Phase 3 (§12). Harness version pinning: per-project lockfile, spawn-time check, `harness-version-watcher`, validation tests.

Per `comp-harness-version-lockfile`, `comp-harness-version-watcher`.

Acceptance: bump a harness binary out-of-band; verify `harness-version-watcher` emits the event, the default `warn` policy records concrete drift while allowing spawns, and `strict` policy refuses new spawns.

Implementation note (2026-05-06): spawn-time Codex lockfile verification lives in `crates/jam-svc-session`; mismatched version or checksum produces `harness-version-drift` / `harness-checksum-drift` under `strict`, and a warning under `warn`. The periodic watcher now exists as `crates/jam-harness-watcher` (`jam-harness-watcher` binary), reading `JAM_HARNESS_LOCKFILE` / `--lockfile-path` and `JAM_CODEX_BIN` / `--codex-bin`, comparing installed Codex version plus SHA-256 to `[harnesses.codex-cli]`, and publishing `journal.harness.version-changed` when drift is detected. `process-compose.yaml` already points `harness-version-watcher` at `/opt/jam/bin/jam-harness-watcher`.

Verification (2026-05-06): unit tests cover matching pins, version drift, checksum drift, and session refusal on version drift. A live smoke with temporary NATS JetStream, `jam-nats-bridge`, a fake Codex binary, and a deliberately stale lockfile produced a `harness.version-changed` journal line.

Runtime note (2026-05-10): `/home/maestro/.jam/config/projects/blueberry-harnesses.lock` now pins Codex CLI `0.129.0` with SHA-256 `baefc109b871e73a7bab298ee19b8bf73c8b647c4f8649a9794fc5db01db17b9` and keeps explicit deferred pins for Claude Code / OpenCode. The live service sets `JAM_HARNESS_LOCKFILE_POLICY=warn`.
