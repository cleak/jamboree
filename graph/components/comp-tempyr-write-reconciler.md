---
id: comp-tempyr-write-reconciler
type: component
status: active
created: 2026-05-06T10:27:17.468744582Z
updated: 2026-05-06T10:27:17.468744582Z
edges:
- target: feat-tempyr-consistency-model
  type: used_by
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
- target: task-tempyr-write-retry-reconciler
  type: used_by
---
Retry lane for Tempyr write side effects (§4.6.4).

Runtime crate: `crates/jam-tempyr-write-reconciler` (`jam-tempyr-write-reconciler` binary). The service subscribes to `journal.tempyr.write-pending`, reads a trusted request file from `JAM_HOME/tempyr-write-requests/`, validates that the request matches the journal event, invokes `tempyr` directly without a shell, and retries with bounded backoff.

Success emits `journal.tempyr.write-confirmed`. Exhaustion emits `journal.tempyr.write-permanently-failed` and publishes `notify.human` with the same trace. Request files are deliberately constrained to the trusted runtime directory and write-shaped Tempyr commands so untrusted journal content cannot become a shell command.
