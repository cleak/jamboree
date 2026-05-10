---
id: comp-orchestrator-jsonl-journal
type: component
status: active
created: 2026-05-04T03:31:36.504213909Z
updated: 2026-05-06T21:15:00Z
edges:
- target: api-orchestrator-journal-envelope
  type: exposes
- target: comp-journal-reconciler
  type: depended_on_by
- target: dec-no-compaction
  type: has_decision
- target: feat-event-schema-versioning
  type: used_by
- target: feat-substrate-services
  type: used_by
- target: principle-journal-is-sacred-no-compaction
  type: constrained_by
---
Append-only JSONL records *what the system did*. Operational events only. Agent reasoning lives in Tempyr's journal (§22), not here.

Contents (§4.4.2): Picker lifecycle, PR/CI events, Maestro tool calls (request, response, success/failure, trace_id), quota state changes, patch events, NATS bus event audit, setup/schema-migration events.

Path layout `~/.jam/journal/YYYY-MM-DD/journal.<group>.jsonl`. Files rotate daily, organized by subject group for human convenience (`tail -f`); programmatic readers use NATS subscriptions.

Envelope (every event):
```jsonl
{"schema_version":1,"event_type":"picker.spawned","event_subtype_version":1,"timestamp":"...","journal_seq":48291,"trace_id":"01HXKJ...","parent_trace_id":"01HXKH...","actor":"jam-svc-session","payload":{...}}
```

`trace_id` placement at top level (not in payload) makes trace queries O(1) per-day-file.

Journal-writer redacts known secret regex patterns at write time (§11.3).

Implementation note (2026-05-06): `jam-nats-bridge` is the live NATS-to-JSONL path. `scripts/smoke-substrate-journal.sh` starts temporary NATS plus `jam-nats-bridge` under process-compose, verifies the `journal` stream and `KV_routing-manifest` stream exist, and confirms a traced `journal.test` message is persisted to `~/.jam/journal/YYYY-MM-DD/journal.test.jsonl` using the envelope timestamp for the date directory.

Production smoke note (2026-05-06): the same script supports `--existing` for
operator verification after `/opt/jam/bin/process-compose` starts the real
substrate. It publishes `journal.test` to the configured NATS URL and verifies
the JSONL entry under the runtime `JAM_HOME` instead of a temporary one.

Maestro-runtime smoke note (2026-05-06): the same script also supports
`--maestro-runtime`, which starts cached NATS and `target/debug/jam-nats-bridge`
as `maestro` and verifies the write under `/home/maestro/.jam/journal` without
requiring a production `/opt/jam/bin` install.
