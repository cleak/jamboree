---
id: comp-skill-suspicion-reconciler
type: component
status: planned
created: 2026-05-04T03:31:44.067889753Z
updated: 2026-05-04T05:03:49.898498185Z
edges:
- target: comp-nats-jetstream
  type: depends_on
- target: comp-tempyr-mcp-client-wrapper
  type: depends_on
- target: dec-skill-suspicion-threshold-3-in-7d
  type: has_decision
- target: feat-failure-handling
  type: used_by
- target: feat-self-improvement
  type: used_by
- target: feat-skill-evolution-pipeline
  type: used_by
- target: feat-substrate-services
  type: used_by
- target: principle-agent-first-bounded-supervision
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
---
Hourly query against Tempyr's `dead_end` corpus (§4.4.6, §7.4, §22.6). Emits `skill.under-suspicion` when a skill accumulates ≥3 entries within 7 days.

```python
hits = tempyr.journal_search(query="", kind="dead_end", since="7d", limit=200)
skill_failures = defaultdict(list)
for entry in hits:
    for tag in entry.tags:
        if tag.startswith("skill:"):
            skill_failures[tag[6:]].append(entry.id)

for skill, entry_ids in skill_failures.items():
    if len(entry_ids) >= 3:
        emit_event("skill.under-suspicion", skill=skill, entries=entry_ids)
```

Maestro sees the event on next wake; decides whether to flag for evolution, deprecate, or ignore. Skills aren't auto-quarantined.

Crate `crates/jam-skill-suspicion/` (bin).