---
id: comp-jam-cli-binary
type: component
status: planned
created: 2026-05-04T03:39:38.455242425Z
updated: 2026-05-04T04:51:50.809480865Z
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