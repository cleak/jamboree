---
id: comp-trace-gap-detector
type: component
status: active
created: 2026-05-04T03:39:57.458187239Z
updated: 2026-05-06T21:33:00Z
edges:
- target: feat-trace-propagation
  type: used_by
- target: principle-one-trigger-one-trace
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
---
`tempyr journal lint` corollary + `jam doctor` check (§22.8, §23.5):

A `trace_id` value that appears in only one entry across all journal sources is suspicious — either trace ended immediately (legal but rare) or trace propagation broke somewhere.

```python
def check_trace_continuity():
    suspect_traces = []
    for trace_id in all_distinct_trace_ids():
        sources = count_appearances(trace_id)  # journal + Tempyr + NATS
        if sources["total"] == 1 and not is_known_single_event_trace_kind(trace_id):
            suspect_traces.append(trace_id)
    if suspect_traces:
        emit_warning("trace propagation may be broken; see traces:", suspect_traces[:10])
```

`is_known_single_event_trace_kind` whitelists known-legal cases (e.g., a `clock-watcher` tick that found nothing wrong emits one event and exits — legitimate single-entry trace).

Also: `Tempyr task node marked in_progress with no recent journal entries from any agent` (existing rule, surfaces hung sessions where the agent never logged).

Implementation note (2026-05-06): `jam doctor` now includes a
`trace-gap-detector` recommended check. It scans `JAM_JOURNAL_ROOT` (or
`JAM_HOME/journal`) JSONL envelopes, counts traced events by `trace_id`, and
warns on unexplained single-entry traces with a remediation to run
`jam trace replay <trace-id>`. Known legal one-event events such as
`clock.unsynced` and `skills.changed` are whitelisted.
