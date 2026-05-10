---
id: comp-harness-version-lockfile
type: component
status: active
created: 2026-05-04T03:34:43.479090176Z
updated: 2026-05-10T00:00:00Z
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
1. **At spawn time.** Adapter checks `version` and `checksum-sha256` against installed binary. Live policy is configurable: `warn` logs and continues on concrete drift, `strict` fails with `harness-version-drift` / `harness-checksum-drift`, and `off` skips concrete comparison. Missing or malformed lockfiles still fail loudly.
2. **On periodic schedule.** `harness-version-watcher` (hourly) compares installed binaries against lockfile.
3. **Validation tests.** Before promoting a harness version: spawn a test Picker, send queue/interrupt/full-stop, verify Tempyr journal session opened+closed.

Auto-update story: most harnesses auto-update by default; override per-harness via lockfile. A `harness-update-candidate` queue at `~/.jam/harness-update-queue.jsonl` accumulates new-version-detected entries; humans review and accept.

Pinning is non-negotiable per §4.5.5 — a Codex CLI version that ships a breaking tool-call protocol change would silently break new spawns.

Implementation note (2026-05-06): Codex lockfile verification is enforced by `jam-svc-session` at spawn time and by `jam-harness-watcher` periodically. Both compare version and SHA-256 for `[harnesses.codex-cli]`.

Runtime note (2026-05-10): the live Blueberry deployment sets `JAM_HARNESS_LOCKFILE_POLICY=warn`, so concrete Codex version/checksum drift is visible without blocking useful work. The current runtime Codex pin is `0.129.0` with SHA-256 `baefc109b871e73a7bab298ee19b8bf73c8b647c4f8649a9794fc5db01db17b9`.
