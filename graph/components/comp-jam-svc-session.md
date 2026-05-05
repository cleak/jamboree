---
id: comp-jam-svc-session
type: component
status: planned
created: 2026-05-04T03:39:30.775848907Z
updated: 2026-05-04T04:54:37.024267655Z
edges:
- target: api-archive-session
  type: exposes
- target: api-inspect-picker
  type: exposes
- target: api-list-active-pickers
  type: exposes
- target: api-purge-session
  type: exposes
- target: api-spawn-picker
  type: exposes
- target: comp-events-toml-and-codegen
  type: depends_on
- target: comp-github-app-client
  type: depends_on
- target: comp-harness-adapter-trait
  type: depends_on
- target: comp-harness-version-lockfile
  type: depends_on
- target: comp-jam-secrets
  type: depends_on
- target: comp-jam-trace-crate
  type: depends_on
- target: comp-nats-jetstream
  type: depends_on
- target: comp-routing-manifest
  type: depends_on
- target: comp-sandbox-backend-trait
  type: depends_on
- target: feat-maestro-tool-surface
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
The session tool service. Subject prefix `tool.session.*`. Crate `crates/jam-svc-session/`. Owns `spawn-picker` (§24.3) and the harness adapter implementations.

Tools: `spawn-picker`, `inspect-picker`, `list-active`, `archive-session`, `purge-session` (§5.2).

Spawn protocol (§24.3):
1. Generate child trace.
2. Verify quota.
3. Worktree creation via NATS request to jam-svc-worktree.
4. Verify harness lockfile (version + checksum match).
5. Sandbox prep via SandboxBackend.
6. Path safety invariants (§6.6).
7. Bootstrap Tempyr journal session for the Picker.
8. Get short-lived GitHub installation token.
9. Get harness-specific secrets via per-harness allowlist.
10. Launch (under multi-user model, via `sudo -n -u picker --preserve-env=...` per security-setup §7.2).
11. Emit `journal.picker.spawned` event with trace_ids in payload.