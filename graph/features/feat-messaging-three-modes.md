---
id: feat-messaging-three-modes
type: feature
status: draft
created: 2026-05-04T03:28:20.801773916Z
updated: 2026-05-06T15:52:03Z
owner: caleb
edges:
- target: api-enqueue-message
  type: exposes
- target: api-full-stop
  type: exposes
- target: api-interrupt-with-message
  type: exposes
- target: comp-harness-adapter-trait
  type: uses
- target: comp-jam-svc-message
  type: uses
- target: comp-jam-svc-supervise
  type: uses
- target: jamboree-v5
  type: child_of
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
- target: the-manager
  type: serves
---
Three message modes corresponding to three execution-state contracts (§5.7):

1. **`enqueue-message`** — deliver at next prompt boundary. Lifecycle: queued → delivered → (optional) acknowledged. Default mode; least disruptive.
2. **`interrupt-with-message`** — cancel current turn at next safe checkpoint and read the message. Capability-gated; only harnesses with `supports_interrupt == true`. Lifecycle: interrupt-requested → interrupt-accepted → delivered. Timeout default 30s.
3. **`full-stop`** — kill Picker process now. SIGTERM with 2s grace, then SIGKILL. Worktree state preserved with `.killed-at-<utc>` marker; `tempyr journal finalize` invoked from cleanup path.

Both source identities (Maestro, human) go through the same tools; tag-on-write distinguishes (`from: human` with optional user-id; `from: maestro` with maestro-session-id). Skill evolution treats human messages as higher-quality supervision signal.

NATS subjects: `picker.<session-id>.msg.queue|interrupt|kill|status`. Strict ordering per session-id; `kill` takes precedence; `queue`/`interrupt` after kill rejected.

Implementation note (2026-05-06): Phase 1 queue/interrupt bus-to-stdin
delivery is active. `jam-svc-message` owns the public tool surface and
`jam-svc-session` subscribes each running Picker to its session-scoped
queue/interrupt subjects, writes framed messages to Picker stdin, and emits
the status lifecycle. Full-stop is active through `tool.session.full-stop`.
Per-harness safe-checkpoint interrupt mechanics remain a harness adapter
refinement.
