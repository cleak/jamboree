---
id: feat-jam-prefix-naming-rule
type: feature
status: active
created: 2026-05-04T03:28:24.299794245Z
updated: 2026-05-04T04:04:58.258030142Z
owner: caleb
edges:
- target: jamboree-v5
  type: child_of
---
Naming rule from CLAUDE.md and §0.0: **prefix where the namespace is shared with the rest of the OS; drop it where the namespace is already `jam`.**

Keep `jam-` prefix for: Rust crates (`jam-svc-observe`, `jam-stall-detector`), process names under process-compose, env vars (`JAM_HOME`, `JAM_TRACE_ID`), system paths (`/etc/jam/`, `/etc/sudoers.d/jam-users`), the future systemd unit (`jam.service`), the ntfy topic.

Drop the prefix for: the CLI binary (`jam`, not `jam-cli`), subcommands (`jam setup`, `jam doctor`), files inside `~/.jam/` (just `maestro.toml`, not `jam-maestro.toml`), skill files, NATS subjects (`journal.picker.spawned`), tool names called by the Maestro (`world-snapshot`), this repo's `scripts/` and `docs/` filenames, audit logs inside an already-prefixed system dir (`/etc/jam/bootstrap.log`).

Three named roles get capitalized prose names: **the Manager**, **the Maestro**, **the Pickers**. Code identifiers follow lowercase Linux convention: `jam_maestro` (Python pkg), `maestro.toml` (config), `maestro/` (source dir), `maestro`/`picker` Linux users.