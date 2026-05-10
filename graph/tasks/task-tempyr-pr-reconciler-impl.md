---
id: task-tempyr-pr-reconciler-impl
type: task
status: done
created: 2026-05-04T04:01:43.179277924Z
updated: 2026-05-06T07:32:48.265368755Z
---
Implement `tempyr-pr-reconciler` — on `pr.merged`, look at touched paths, query Tempyr for nodes referencing them, emit `tempyr.update-candidate`.

Per `comp-tempyr-pr-reconciler`, `feat-tempyr-consistency-model`.

Implementation note (2026-05-06): `crates/jam-tempyr-pr-reconciler` now subscribes to traced `journal.pr.merged`, parses the merge payload's `touched_paths`, scans the configured Tempyr graph for Markdown node files that reference those paths, and emits traced `journal.tempyr.update-candidate` events with `source: auto`. The reconciler also supports `--once` journal replay and `--max-events` for smoke tests; it never edits Tempyr graph nodes directly.

Live smoke (2026-05-06): temporary NATS root `/tmp/jam-tempyr-pr-reconciler-smoke-Ihjqzv` published a traced `pr.merged` for `cleak/blueberry#999` with touched path `crates/foo/src/lib.rs`. The graph fixture had `comp-smoke` referencing that path. `jam-tempyr-pr-reconciler --graph-dir ... --max-events 1` emitted exactly one `journal.tempyr.jsonl` entry: `tempyr.update-candidate`, trace `01KQY2NSDFD5GQAWJVWQJZEJRM`, actor `jam-tempyr-pr-reconciler`, payload `node_id: comp-smoke`, `source: auto`, reason `cleak/blueberry#999 merged abcdef123456 touching crates/foo/src/lib.rs`.
