---
id: comp-jam-svc-knowledge
type: component
status: active
created: 2026-05-04T03:39:34.201855192Z
updated: 2026-05-06T21:20:00Z
edges:
- target: api-query-session-store
  type: exposes
- target: api-query-tempyr
  type: exposes
- target: api-read-skills
  type: exposes
- target: api-record-tempyr-update-candidate
  type: exposes
- target: api-tempyr-journal-blame
  type: exposes
- target: api-tempyr-journal-range
  type: exposes
- target: api-tempyr-journal-search
  type: exposes
- target: comp-events-toml-and-codegen
  type: depends_on
- target: comp-jam-trace-crate
  type: depends_on
- target: comp-nats-jetstream
  type: depends_on
- target: comp-routing-manifest
  type: depends_on
- target: comp-session-store
  type: depends_on
- target: comp-tempyr-mcp-client-wrapper
  type: depends_on
- target: constraint-inotify-watches-524k
  type: constrained_by
- target: feat-live-update-flows
  type: used_by
- target: feat-maestro-tool-surface
  type: used_by
- target: feat-record-learning
  type: used_by
- target: feat-self-improvement
  type: used_by
- target: feat-tool-services-out-of-process
  type: used_by
- target: principle-decoupled-processes-bus
  type: constrained_by
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
---
Knowledge / Tempyr / session store tool service. Subject prefix `tool.knowledge.*`. Crate `crates/jam-svc-knowledge/`.

Tools (§5.5):
- `query-tempyr(query, scope?)`
- `query-session-store(query, time-range?)`
- `read-skills(scope?)` — relevance-scoped skill loading (§4.1.3)
- `record-tempyr-update-candidate(candidate)`
- `tempyr-journal-search(query, kind?, agent?, since?)`
- `tempyr-journal-blame(file-path)`
- `tempyr-journal-range(rev-range)`

Owns the inotify watcher on the skills dir; emits `skills.changed{file_path}` (§21.4).

Implementation note (2026-05-06): first crate slice landed at `crates/jam-svc-knowledge/` for skills hot-edit watching only. The broader `tool.knowledge.*` surface remains planned; the watcher responsibility from §21.4 is implemented and smoke-tested by `task-skill-watcher-inotify`.
