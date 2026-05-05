---
id: comp-trace-gap-detector
type: component
status: planned
created: 2026-05-04T03:39:57.458187239Z
updated: 2026-05-04T04:24:56.039223035Z
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