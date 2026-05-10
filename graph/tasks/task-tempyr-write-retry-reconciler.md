---
id: task-tempyr-write-retry-reconciler
type: task
status: done
created: 2026-05-04T04:01:33.572030162Z
updated: 2026-05-06T10:27:17.468744582Z
edges:
- target: comp-tempyr-write-reconciler
  type: uses
---
Implement Tempyr write retry: `[100ms, 500ms, 2s, 10s, 60s]` backoff. After exhaustion, emit `tempyr.write-permanently-failed` and ntfy human.

Per `feat-tempyr-consistency-model`.

Implementation note (2026-05-06): added `crates/jam-tempyr-write-reconciler` and a disabled `tempyr-write-reconciler` process-compose entry. The reconciler subscribes to `journal.tempyr.write-pending`, loads trusted request files from `JAM_HOME/tempyr-write-requests/`, invokes `tempyr` without a shell using a bounded allowlist of write-shaped commands, retries with default `[100ms, 500ms, 2s, 10s, 60s]`, emits `journal.tempyr.write-confirmed` on success, and emits `journal.tempyr.write-permanently-failed` plus `notify.human` after exhaustion. Live smoke used temporary NATS + `jam-nats-bridge` + fake `tempyr`: one malformed `/tmp` worktree request produced `tempyr.write-permanently-failed` with `attempts=0`, then a transient fake failure retried and produced `tempyr.write-confirmed` with `attempts=2` in `journal.tempyr.jsonl`.
