---
id: comp-harness-version-watcher
type: component
status: active
created: 2026-05-04T03:31:45.544388182Z
updated: 2026-05-10T00:00:00Z
edges:
- target: comp-harness-version-lockfile
  type: depends_on
- target: comp-nats-jetstream
  type: depends_on
- target: feat-failure-handling
  type: used_by
- target: feat-substrate-services
  type: used_by
- target: principle-agent-first-bounded-supervision
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
---
Diffs installed harness binaries vs lockfile every hour (§4.4.6, §4.5.5, §21.3). Emits `harness.version-changed` events on drift.

Patch agent picks these up. Spawn-time behavior is controlled by
`JAM_HARNESS_LOCKFILE_POLICY`: the current live default warns and continues,
while `strict` refuses new spawns until drift is acknowledged (§10.2).

Lockfile path: `~/.jam/config/projects/<project>-harnesses.lock`.

Crate `crates/jam-harness-watcher/` (bin).

Implementation note (2026-05-06): crate is implemented as `crates/jam-harness-watcher` with binary `jam-harness-watcher`. It supports `--once`, `--max-ticks`, `--interval-secs`, `--lockfile-path`, and `--codex-bin`; defaults match the Blueberry harness lockfile under `JAM_HOME`. Drift publishes `journal.harness.version-changed` with expected and installed version/checksum fingerprints.
