---
id: comp-harness-version-lockfile
type: component
status: planned
created: 2026-05-04T03:34:43.479090176Z
updated: 2026-05-04T04:47:41.376725065Z
edges:
- target: comp-harness-version-watcher
  type: depended_on_by
- target: comp-jam-svc-session
  type: depended_on_by
- target: feat-picker-layer-three-tier
  type: used_by
- target: principle-failure-surfaces-immediately
  type: constrained_by
---
Per-project lockfile, version-controlled in the orchestrator config repo (§4.5.5).

```toml
# ~/.jam/config/projects/blueberry-harnesses.lock
[harnesses.codex-cli]
version = "0.42.1"
checksum-sha256 = "abc123..."
last-validated = "..."
validation-tests-passed = ["spawn", "interrupt", "full-stop", "session-resume", "tempyr-bootstrap"]
```

Three enforcement points:
1. **At spawn time.** Adapter checks `version` and `checksum-sha256` against installed binary. Mismatch → spawn fails with `harness-version-drift` event.
2. **On periodic schedule.** `harness-version-watcher` (hourly) compares installed binaries against lockfile.
3. **Validation tests.** Before promoting a harness version: spawn a test Picker, send queue/interrupt/full-stop, verify Tempyr journal session opened+closed.

Auto-update story: most harnesses auto-update by default; override per-harness via lockfile. A `harness-update-candidate` queue at `~/.jam/harness-update-queue.jsonl` accumulates new-version-detected entries; humans review and accept.

Pinning is non-negotiable per §4.5.5 — a Codex CLI version that ships a breaking tool-call protocol change would silently break new spawns.