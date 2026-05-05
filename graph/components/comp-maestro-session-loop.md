---
id: comp-maestro-session-loop
type: component
status: planned
created: 2026-05-04T03:31:29.935848525Z
updated: 2026-05-04T04:33:28.753355119Z
edges:
- target: comp-maestro-tempyr-journal-anchor
  type: depends_on
- target: comp-routing-manifest
  type: depends_on
- target: feat-input-budget-management
  type: used_by
- target: feat-maestro-orchestration-loop
  type: used_by
- target: principle-episodic-maestro-sessions
  type: constrained_by
- target: principle-provider-agnostic-everywhere
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
---
The episodic session loop (§4.1.2, §24.2). For each wake:

1. Load relevant skills via `read-skills(scope)`.
2. Call `world-snapshot(task_id, fresh=True)` if task-related.
3. Build messages (skills + snapshot + wake event context).
4. Call `backend.respond(MaestroRequest)` with `reasoning_effort=medium`.
5. While response has tool calls: dispatch each via NATS request-reply, append result, respond again.
6. On done/budget/interrupt/fatal: close session, finalize Tempyr journal (`tempyr journal finalize`), publish `journal.maestro.session-ended`.

Tool call dispatch reads current routing manifest from NATS KV (cached for session, refreshed on `routing-manifest.updated` events), constructs subject from `<service.subject_prefix>.<method>`, sends NATS request with trace headers, awaits reply with timeout (default 30s/tool).