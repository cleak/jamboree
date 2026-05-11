# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this directory is

`/home/caleb/jamboree/` is the home for **Jamboree** — the multi-coding-agent orchestrator that drives Caleb's Bevy/Rust voxel game *Blueberry*. This is a **monorepo**: the spec, the bootstrap scripts, and the orchestrator's implementation (Rust + Python + SolidJS) all live in the same git checkout. See `docs/layout.md` for the layout decision and full directory map.

Contents:

- `docs/proposal-v5.md` — implementation-ready architecture spec (4,400+ lines, §0–§24). Sections cite each other extensively (`§4.6.1`-style); follow the cross-references rather than re-deriving design.
- `docs/security-setup.md` — v5 addendum for the multi-user isolation model (`maestro` substrate user, `picker` Picker user).
- `docs/layout.md` — monorepo decision and directory layout reference.
- `scripts/` — idempotent bash scripts for system bootstrap (users, CLI tools, keyring, secrets seed). Prerequisite to `jam setup`.
- `crates/`, `maestro/`, `ui/` — implementation directories, scaffolded as Phase 3 work begins.

Source-of-truth lives here. **Runtime** deploys to `/home/maestro/.jam/` per `security-setup.md` §7.1; build artifacts cross the user boundary via `jam patch apply` (spec §21.6).

Sibling directories on this machine that are relevant:
- `/home/caleb/blueberry/` — the Bevy/Rust voxel game (Jamboree's initial target). Has its own `CLAUDE.md`/`AGENTS.md`.
- `/home/caleb/autoberry/` and `/home/caleb/autoberry-worktrees/` — an earlier iteration of orchestration for Blueberry (different design; do not assume v5 conventions apply there).
- `/home/caleb/tempyr/` — Caleb's knowledge-graph tool that Jamboree integrates with (proposal §4.6, §22).

## Naming — only three roles get a name

The Jamboree theme is musical (many agents jamming in parallel; harvest preserved as durable knowledge). Three roles are named; everything else stays generic.

| Role | Jamboree name | Who/what |
|---|---|---|
| Human operator | **the Manager** | Books gigs (queues tasks), funds the budget, gets paged when things go wrong, signs off on encores (PR merges) |
| Orchestrator agent (Python) | **the Maestro** | Calls every tune, cues every Picker, runs the show in real time |
| Sandboxed coding agents | **the Pickers** | Berry pickers + guitar pickers + task-pickers; one per task in its own Booth |

Linux user mapping:

| Identity | Linux user | UID |
|---|---|---|
| Caleb (human) | `caleb` | 1000 |
| Substrate (runs the Maestro and all backline services) | `maestro` | 2000 |
| Pickers | `picker` | 2001 |

Use **the Maestro** and **the Pickers** consistently. Earlier drafts used lowercase "conductor" and "workers" for the same concepts; those have been collapsed to a single name per role. Code identifiers follow lowercase Linux convention: `jam_maestro` (Python package), `maestro.toml` (config), `maestro/` (source dir), and the `maestro` / `picker` Linux users.

## When to keep / drop the `jam-` prefix

Established convention: prefix where the namespace is shared with the rest of the OS; drop it where the surrounding context already names it.

**Keep the prefix** for: Rust crates (`jam-svc-observe`, `jam-stall-detector`), process names under process-compose, env vars (`JAM_HOME`, `JAM_TRACE_ID`), system paths (`/etc/jam/`, `/etc/sudoers.d/jam-users`), the future systemd unit (`jam.service`), and the ntfy topic.

**Drop the prefix** for: the CLI binary (`jam`, not `jam-cli`), subcommands (`jam setup`, `jam doctor`), files inside `~/.jam/` (just `maestro.toml`, not `jam-maestro.toml`), skill files, NATS subjects (`journal.picker.spawned`, not `jam.journal.…`), tool names called by the Maestro (`world-snapshot`, not `jam-world-snapshot`), this repo's `scripts/` and `docs/` filenames, and audit logs inside an already-prefixed system dir (`/etc/jam/bootstrap.log`).

Rule: **prefix where the namespace is shared with the rest of the OS; drop it where the namespace is already `jam`.**

## What you'll typically be asked to do here

1. **Edit the spec or addendum.** Keep §-anchors stable; many sections reference others by `§N.M`. The v5 changelog is §15. Naming is introduced at §0.0 (top of the spec).
2. **Modify the bootstrap script.** It must stay idempotent, support `--dry-run` and `--verify-only`, and follow the "fail loudly with specific remediation" pattern from spec §2.12.
3. **Add new docs or scripts** that will eventually become part of the Jamboree runbook.

Implementation work (Rust services, Python Maestro, UI) happens in this same monorepo per `docs/layout.md`. Scaffolding goes under `crates/`, `maestro/`, and `ui/` — see the layout doc for the full tree before adding new top-level directories.

## Running the bootstrap script

```bash
sudo ./scripts/bootstrap-users.sh                 # interactive; uses $SUDO_USER
sudo ./scripts/bootstrap-users.sh --user caleb    # explicit
sudo ./scripts/bootstrap-users.sh --dry-run       # preview only
sudo ./scripts/bootstrap-users.sh --verify-only   # audit existing setup, no changes
```

After it runs, GPG/`pass` initialization for `maestro` is a manual one-time step documented in `security-setup.md` §5. The script deliberately does not do this because key-generation choices are user-specific.

If you change the script, validate generated sudoers with `visudo -cf <tempfile>` before installing — the script already does this; preserve the pattern. The script also writes an audit record to `/etc/jam/bootstrap.log` that `jam doctor` reads as the verified-good baseline.

## Running the CLI-tools installer

```bash
sudo ./scripts/install-cli-tools.sh               # codex + claude-code per user, plus daily auto-update cron
sudo ./scripts/install-cli-tools.sh --dry-run
sudo ./scripts/install-cli-tools.sh --verify-only
```

Run after `bootstrap-users.sh` has succeeded. Installs `@openai/codex` and `@anthropic-ai/claude-code` per-user for `caleb`, `maestro`, and `picker` — never as root, because both tools' auto-updaters require write access to their install directory. Wires up `/etc/cron.d/jam-cli-update` to run `cli-tools-update.sh` once a day for each user (4:15 / 4:30 / 4:45 AM, staggered). See `security-setup.md` §4.5.

## Deploying a tool service (`jam deploy`)

`jam deploy <service>` is the one-shot path from a working-tree edit to a live service. It does three things:

1. `cargo build --release -p jam-svc-<service>` in this checkout.
2. Computes a version string (workspace `version` with `-<short-sha>-dirty` suffix if the tree is dirty; overridable with `--version`).
3. Publishes `patch.staged` over NATS pointing at `target/release/jam-svc-<service>`. `--from <path>` skips the build and uses a pre-built binary.

The `patch-agent` (process-compose service, runs as `maestro`) consumes `patch.staged` and runs §20.3:

- copies the binary across the user boundary into `/home/maestro/.jam/bin/jam-svc-<service>-<version>`,
- starts a candidate on a versioned subject prefix (`tool.<service>.v<version-slug>`),
- gates on ping + smoke + `jam doctor` + recent-failed-events,
- writes the new revision into the `routing-manifest` NATS KV bucket atomically,
- drains the previous candidate.

The CLI waits for `patch.confirmed` or `patch.failed`. On failure it reports an incident under `/home/maestro/.jam/incidents/incident-<ULID>/` with `summary.json`, `health-post-apply.json`, `rollback-command.json`, `llm-diagnosis.json`, and the trailing 1000 journal events.

**Where binaries live:**

- Runtime, per-version patches: `/home/maestro/.jam/bin/jam-svc-<service>-<version>` — written by patch-agent during `apply`. The routing manifest points at one of these.
- Canonical first-install location: `/opt/jam/bin/jam-<name>` — written by `scripts/install-substrate.sh`. Used for fresh substrate brings, including for `patch-agent` itself.

Caleb has NOPASSWD sudo for installing into `/opt/jam/bin/` via the tightly-scoped `scripts/jam-install-bin` wrapper (installed to `/opt/jam/sbin/jam-install-bin` by `bootstrap-users.sh`, whitelisted in the generated sudoers as `Cmnd_Alias JAM_INSTALL_BIN`). The wrapper refuses sources outside `target/release/` and dest names that don't start with `jam-`.

**Gotcha — first-patch rollback.** If a post-apply check fails on the *first* patch ever applied to a service, the mechanical rollback fails with `current routing manifest has no previous_manifest_id` (there's no previous revision to revert to). The atomic swap has already happened by that point, so the new binary is live; only the rollback-on-failure path is broken. Treat reported failures on a first-deploy as "verify the new code is actually serving requests" (e.g., `strings $bin | grep <new-symbol>` and `jam health ping <service>`) before assuming the deploy didn't land.

**Process-compose surfaces (run as `maestro`):**

```bash
sudo -u maestro /opt/jam/bin/process-compose project update \
    -f /home/caleb/jamboree/process-compose.yaml \
    -u /home/maestro/.jam/process-compose.sock -U
sudo -u maestro /opt/jam/bin/process-compose process get patch-agent \
    -u /home/maestro/.jam/process-compose.sock -U
sudo -u maestro /opt/jam/bin/process-compose process restart patch-agent \
    -u /home/maestro/.jam/process-compose.sock -U
```

The patch-agent must be running for `jam deploy` to make progress; the CLI will time out otherwise.

## Load-bearing principles to preserve

When editing the spec or writing related code, these v5 principles are non-negotiable (cite by number in code comments per §2):

- **§2.1 More observable, not more deterministic.** The Maestro starts every decision from `world-snapshot`, not state machines.
- **§2.3 Structure lives in tools, not policy.** No `merge-pr` tool exists; only `request-human-merge`. Invariants are tool-shape, not runtime checks.
- **§2.7 Untrusted content cannot issue commands.** Use `Untrusted<String>` newtype; never format untrusted bodies into shell/system-prompt strings.
- **§2.8 Provider-agnostic everywhere.** LiteLLM for LLMs, search-router for search, harness-adapter trait for Pickers, sandbox-backend trait for execution. Never hardcode a provider.
- **§2.12 Failure surfaces immediately.** Components refuse to start with broken config; emit `*.failed` events with `error_kind`, `detail`, `trace_id`, and remediation hint. No silent degradation.
- **§2.13 Tracing chains, end to end.** Every NATS message, tool call, and journal entry carries `trace_id`. NATS publish wrapper rejects publishes without it.
- **§2.14 Native FS only.** Refuse `/mnt/c/`, `/cygdrive/`, etc. Linux native filesystem under `/home/<user>/`. The `is_windows_mount` check in §6.6 is the canonical implementation.

## Multi-user model (security-setup.md)

The system runs across three identities; this caleb-side checkout is the entry point for setting that up:

| User | Role | Lives in |
|---|---|---|
| `caleb` (the **Manager**) | Human; runs the `jam` CLI; edits skills and Tempyr nodes | `/home/caleb/` (mode 751) |
| `maestro` | Substrate (NATS, the Maestro process, all `jam-svc-*` services, UI server) | `/home/maestro/.jam/` |
| `picker` | Sandboxed Picker processes | `/home/picker/workers/<task-id>/` |

Sudoers gives NOPASSWD transitions `caleb→maestro`, `caleb→picker`, `maestro→picker`. Convenience-first: defends against prompt-injection-driven exfiltration and rogue Pickers, **not** against an attacker who already has caleb's shell.

Shared dirs (`~/code/blueberry-tempyr-live/`, `~/code/jam-skills/`) use mode `2770` with group `maestro` so the setgid bit propagates group ownership to new files.

## Repo conventions

- **Naming:** kebab-case for tool names, NATS subjects, event types, and skill scopes. Match existing.
- **Markdown:** GitHub-flavored. Code blocks use language tags. Section anchors are stable (`§4.5.1`-style cross-references depend on them).
- **Bash scripts:** `set -euo pipefail`. Mirror the bootstrap script's `pass`/`fail`/`info`/`warn`/`die` helpers and "Fix:" remediation block style — this is the same pattern `jam doctor` uses.
- **Dates in docs:** UTC, RFC 3339 where precise; `YYYY-MM-DD` in headers. The spec currently dates to 2026-05-03.
- **Cargo workspace, Python package, and `package.json`** all live in this monorepo under `crates/`, `maestro/`, and `ui/` respectively. See `docs/layout.md` for the full directory tree.

## When the user says "the bulk of the work happens under other accounts"

They mean: at *runtime*, most code execution happens as `maestro` (the orchestrator process and its services) and `picker` (sandboxed Picker processes operating in worktrees under `/home/picker/workers/<task-id>/`). *Source-of-truth* still lives in this caleb-owned monorepo — design (`docs/`), bootstrap (`scripts/`), and implementation (`crates/`, `maestro/`, `ui/`) all here. Build artifacts cross the user boundary via `jam patch apply` (spec §21.6); source-of-truth never moves.
