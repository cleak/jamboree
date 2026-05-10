---
id: comp-tempyr-mcp-client-wrapper
type: component
status: active
created: 2026-05-04T03:34:45.952494356Z
updated: 2026-05-06T21:18:00Z
edges:
- target: api-tempyr-journal-entry-kinds
  type: exposes
- target: comp-jam-svc-knowledge
  type: depended_on_by
- target: comp-jam-svc-observe
  type: depended_on_by
- target: comp-research-completion-handler
  type: depended_on_by
- target: comp-skill-suspicion-reconciler
  type: depended_on_by
- target: comp-tempyr-pr-reconciler
  type: depended_on_by
- target: feat-record-learning
  type: used_by
- target: feat-tempyr-consistency-model
  type: used_by
- target: feat-tempyr-knowledge-and-journal
  type: used_by
---
Wraps Tempyr's MCP client to auto-tag every journal entry with `trace:<id>` and `parent-trace:<id>` (§23.3.4):

```python
def journal_log(kind, fields, tags=None, ctx=current_trace_ctx()):
    tags = list(tags or [])
    tags.append(f"trace:{ctx.trace_id}")
    if ctx.parent_trace_id:
        tags.append(f"parent-trace:{ctx.parent_trace_id}")
    return tempyr.journal_log(kind=kind, fields=fields, tags=tags)
```

Direct CLI use of `tempyr journal log` from outside the orchestrator (e.g., a human running it manually) won't auto-tag — those entries are manually-taggable.

Lives at `maestro/src/jam_maestro/tempyr_journal.py`. Also exposes `journal_search`/`journal_blame`/`journal_range` wrappers.

Implementation note (2026-05-06): the Python wrapper is active for Maestro
journaling and the generated knowledge tools. `CliTempyrJournal` writes
structured entries with trace tags, and `tempyr_journal_search` /
`tempyr_journal_blame` / `tempyr_journal_range` provide typed query wrappers.
