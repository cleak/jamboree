---
id: comp-maestro-session-loop
type: component
status: active
created: 2026-05-04T03:31:29.935848525Z
updated: 2026-05-06T13:21:57Z
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

Implementation status (2026-05-06): the Phase 1 `world-snapshot` call path now uses `RoutingManifestRouter` backed by NATS KV, so routed observe calls use the current manifest prefix when present. Full model-driven recursive tool dispatch is still planned.

Implementation note (2026-05-06): session decisions now include the first planned dispatch result. After loading skills and calling `world-snapshot`, the Maestro calls the quota-aware dispatch policy and records either `dispatch-ready:<harness>` with a planned `spawn-picker` request or a typed blocked reason. Runtime `run-task`, `wake-once`, and `listen` loops now install a routed NATS session client, so a dispatch choice becomes a traced `tool.session.spawn-picker` call and the returned Picker handle is recorded on the session decision. Unit coverage keeps planner-only loops available for tests while proving routed `session.spawn-picker`, successful spawn recording, and typed spawn-error blocking.

Smoke note (2026-05-06): a temporary NATS run with `jam-svc-observe`, `jam-svc-session`, a fake pinned Codex binary, and a fake worktree responder exercised the Python `MaestroSessionLoop` with real `NatsObserveClient` and `NatsSessionClient`. The session returned `decision=spawned:codex-cli`, recorded a `codex-cli:*` Picker handle, and the spawned fake Codex process wrote `.jam/codex-events.jsonl` in the task worktree.
