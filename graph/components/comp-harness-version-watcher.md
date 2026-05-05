---
id: comp-harness-version-watcher
type: component
status: planned
created: 2026-05-04T03:31:45.544388182Z
updated: 2026-05-04T04:47:41.376724621Z
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

Patch agent picks these up. Maestro sees the event and refuses new spawns until acknowledged (§10.2).

Lockfile path: `~/.jam/config/projects/<project>-harnesses.lock`.

Crate `crates/jam-harness-watcher/` (bin).