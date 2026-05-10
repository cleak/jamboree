---
id: comp-jam-cli-binary
type: component
status: active
created: 2026-05-04T03:39:38.455242425Z
updated: 2026-05-06T19:03:50Z
edges:
- target: comp-jam-secrets
  type: depends_on
- target: comp-jam-trace-crate
  type: depends_on
- target: feat-jam-cli
  type: used_by
---
The `jam` CLI binary (§11.1). Crate `crates/jam-cli/`. User-facing commands: see `feat-jam-cli`.

Per-command commands open a root trace via `TraceCtx::new_root(origin_kind, origin_summary)` and publish a corresponding journal event (e.g., `journal.task.requested`, `journal.patch.staged`).

CLI runs as caleb, talks to NATS (owned by maestro). Sudoers rule lets caleb's CLI read `pass` via `sudo -n -u maestro -i pass show jam/nats/token` without password.

Implementation note (2026-05-06): NATS-using CLI commands resolve auth with `NATS_TOKEN` first, then the maestro pass bridge for `jam/nats/token`, then no token for unauthenticated local development NATS.

Implementation note (2026-05-06): `jam quota show` now calls `tool.observe.query-quota` over NATS, carrying a root trace and printing `harness/window_kind` quota states for Manager inspection, including reset cadence, API budget, usage, and price-event columns when present. `jam quota recalibrate` publishes root-traced quota correction events to `journal.quota.*`.

Implementation note (2026-05-06): `jam tempyr canonical-worktree recreate` now handles the first install case where the local `tempyr-live` branch does not exist yet. It creates the branch from `JAM_TEMPYR_BASE_REF` when set, otherwise from a detected remote/local trunk ref.
