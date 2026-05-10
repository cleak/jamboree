---
id: api-orchestrator-journal-envelope
type: api_surface
status: stable
created: 2026-05-04T03:53:51.256953276Z
updated: 2026-05-06T21:29:02Z
edges:
- target: comp-orchestrator-jsonl-journal
  type: exposed_by
- target: feat-substrate-services
  type: exposed_by
---
JSONL envelope for every orchestrator journal entry (§4.4.2):

```jsonl
{"schema_version":1,"event_type":"picker.spawned","event_subtype_version":1,"timestamp":"...","journal_seq":48291,"trace_id":"01HXKJ...","parent_trace_id":"01HXKH...","actor":"jam-svc-session","payload":{...}}
```

Fields:
- `schema_version`: envelope version (1).
- `event_type`: kebab-case dotted name.
- `event_subtype_version`: per-event-type version. Bumps on additive changes; breaking changes get new event types.
- `timestamp`: UTC RFC 3339 nanosecond, sourced at producing service.
- `journal_seq`: monotonic sequence assigned by journal writer.
- `trace_id`: required (§23). Top-level for O(1) trace queries.
- `parent_trace_id`: optional, child traces.
- `actor`: service name | Maestro session ID | `human:<user-id>`.
- `payload`: event-specific shape, validated against generated JSON schema.

Implementation note (2026-05-06): `jam-events::EventEnvelope` is the shared envelope type used by publishers, and `jam-nats-bridge` persists traced `journal.*` messages as rotated JSONL under the runtime journal root.
