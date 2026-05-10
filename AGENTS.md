# Jamboree Agent Guide

This file provides guidance to coding agents when working in this repository.

## What This Directory Is

`/home/caleb/jamboree/` is the home for **Jamboree**: the multi-coding-agent orchestrator that drives Caleb's Bevy/Rust voxel game *Blueberry*. This is a **monorepo**: the spec, bootstrap scripts, and orchestrator implementation (Rust + Python + SolidJS) all live in the same git checkout. See `docs/layout.md` for the layout decision and full directory map.

Contents:

- `docs/proposal-v5.md` - implementation-ready architecture spec (4,400+ lines, §0-§24). Sections cite each other extensively (`§4.6.1`-style); follow the cross-references rather than re-deriving design.
- `docs/security-setup.md` - v5 addendum for the multi-user isolation model (`maestro` substrate user, `picker` Picker user).
- `docs/layout.md` - monorepo decision and directory layout reference.
- `scripts/` - idempotent bash scripts for system bootstrap (users, CLI tools, keyring, secrets seed). Prerequisite to `jam setup`.
- `crates/`, `maestro/`, `ui/` - implementation directories, scaffolded as Phase 3 work begins.
- `graph/`, `.tempyr/` - Tempyr knowledge graph nodes, schema, config, render templates, and interview sessions.

Source-of-truth lives here. **Runtime** deploys to `/home/maestro/.jam/` per `security-setup.md` §7.1; build artifacts cross the user boundary via `jam patch apply` (spec §21.6).

Relevant sibling directories on this machine:

- `/home/caleb/blueberry/` - the Bevy/Rust voxel game (Jamboree's initial target). Has its own `CLAUDE.md`/`AGENTS.md`.
- `/home/caleb/autoberry/` and `/home/caleb/autoberry-worktrees/` - an earlier iteration of orchestration for Blueberry (different design; do not assume v5 conventions apply there).
- `/home/caleb/tempyr/` - Caleb's knowledge-graph tool that Jamboree integrates with (proposal §4.6, §22).

## Naming: Only Three Roles Get A Name

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

## When To Keep Or Drop The `jam-` Prefix

Established convention: prefix where the namespace is shared with the rest of the OS; drop it where the surrounding context already names it.

Keep the prefix for: Rust crates (`jam-svc-observe`, `jam-stall-detector`), process names under process-compose, env vars (`JAM_HOME`, `JAM_TRACE_ID`), system paths (`/etc/jam/`, `/etc/sudoers.d/jam-users`), the future systemd unit (`jam.service`), and the ntfy topic.

Drop the prefix for: the CLI binary (`jam`, not `jam-cli`), subcommands (`jam setup`, `jam doctor`), files inside `~/.jam/` (just `maestro.toml`, not `jam-maestro.toml`), skill files, NATS subjects (`journal.picker.spawned`, not `jam.journal.*`), tool names called by the Maestro (`world-snapshot`, not `jam-world-snapshot`), this repo's `scripts/` and `docs/` filenames, and audit logs inside an already-prefixed system dir (`/etc/jam/bootstrap.log`).

Rule: **prefix where the namespace is shared with the rest of the OS; drop it where the namespace is already `jam`.**

## Typical Work In This Repo

1. **Edit the spec or addendum.** Keep §-anchors stable; many sections reference others by `§N.M`. The v5 changelog is §15. Naming is introduced at §0.0 near the top of the spec.
2. **Modify bootstrap scripts.** They must stay idempotent, support `--dry-run` and `--verify-only`, and follow the "fail loudly with specific remediation" pattern from spec §2.12.
3. **Add docs or scripts** that will eventually become part of the Jamboree runbook.
4. **Implement Jamboree components** in this monorepo under `crates/`, `maestro/`, and `ui/`. See `docs/layout.md` before adding new top-level directories.
5. **Maintain Tempyr graph nodes** when product or technical design knowledge needs to become durable.

## Running The Bootstrap Script

```bash
sudo ./scripts/bootstrap-users.sh                 # interactive; uses $SUDO_USER
sudo ./scripts/bootstrap-users.sh --user caleb    # explicit
sudo ./scripts/bootstrap-users.sh --dry-run       # preview only
sudo ./scripts/bootstrap-users.sh --verify-only   # audit existing setup, no changes
```

After it runs, GPG/`pass` initialization for `maestro` is a manual one-time step documented in `security-setup.md` §5. The script deliberately does not do this because key-generation choices are user-specific.

If you change the script, validate generated sudoers with `visudo -cf <tempfile>` before installing. The script already does this; preserve the pattern. The script also writes an audit record to `/etc/jam/bootstrap.log` that `jam doctor` reads as the verified-good baseline.

## Running The CLI Tools Installer

```bash
sudo ./scripts/install-cli-tools.sh               # codex + claude-code per user, plus daily auto-update cron
sudo ./scripts/install-cli-tools.sh --dry-run
sudo ./scripts/install-cli-tools.sh --verify-only
```

Run after `bootstrap-users.sh` has succeeded. Installs `@openai/codex` and `@anthropic-ai/claude-code` per-user for `caleb`, `maestro`, and `picker`, never as root, because both tools' auto-updaters require write access to their install directory. Wires up `/etc/cron.d/jam-cli-update` to run `cli-tools-update.sh` once a day for each user (4:15 / 4:30 / 4:45 AM, staggered). See `security-setup.md` §4.5.

## Load-Bearing Principles To Preserve

When editing the spec or writing related code, these v5 principles are non-negotiable. Cite by number in code comments per §2 when relevant.

- **§2.1 More observable, not more deterministic.** The Maestro starts every decision from `world-snapshot`, not state machines.
- **§2.3 Structure lives in tools, not policy.** No `merge-pr` tool exists; only `request-human-merge`. Invariants are tool-shape, not runtime checks.
- **§2.7 Untrusted content cannot issue commands.** Use `Untrusted<String>` newtype; never format untrusted bodies into shell/system-prompt strings.
- **§2.8 Provider-agnostic everywhere.** LiteLLM for LLMs, search-router for search, harness-adapter trait for Pickers, sandbox-backend trait for execution. Never hardcode a provider.
- **§2.12 Failure surfaces immediately.** Components refuse to start with broken config; emit `*.failed` events with `error_kind`, `detail`, `trace_id`, and remediation hint. No silent degradation.
- **§2.13 Tracing chains, end to end.** Every NATS message, tool call, and journal entry carries `trace_id`. NATS publish wrapper rejects publishes without it.
- **§2.14 Native FS only.** Refuse `/mnt/c/`, `/cygdrive/`, etc. Linux native filesystem under `/home/<user>/`. The `is_windows_mount` check in §6.6 is the canonical implementation.

## Multi-User Model

The system runs across three identities; this caleb-side checkout is the entry point for setting that up.

| User | Role | Lives in |
|---|---|---|
| `caleb` (the **Manager**) | Human; runs the `jam` CLI; edits skills and Tempyr nodes | `/home/caleb/` (mode 751) |
| `maestro` | Substrate (NATS, the Maestro process, all `jam-svc-*` services, UI server) | `/home/maestro/.jam/` |
| `picker` | Sandboxed Picker processes | `/home/picker/workers/<task-id>/` |

Sudoers gives NOPASSWD transitions `caleb -> maestro`, `caleb -> picker`, `maestro -> picker`. Convenience-first: defends against prompt-injection-driven exfiltration and rogue Pickers, **not** against an attacker who already has caleb's shell.

Shared dirs (`~/code/blueberry-tempyr-live/`, `~/code/jam-skills/`) use mode `2770` with group `maestro` so the setgid bit propagates group ownership to new files.

When the user says "the bulk of the work happens under other accounts", they mean runtime execution happens mostly as `maestro` and `picker`. Source-of-truth still lives in this caleb-owned monorepo: design (`docs/`), bootstrap (`scripts/`), implementation (`crates/`, `maestro/`, `ui/`), and graph (`graph/`, `.tempyr/`). Build artifacts cross the user boundary via `jam patch apply` (spec §21.6); source-of-truth never moves.

## Repo Conventions

- **Naming:** kebab-case for tool names, NATS subjects, event types, and skill scopes. Match existing conventions.
- **Markdown:** GitHub-flavored. Code blocks use language tags. Section anchors are stable (`§4.5.1`-style cross-references depend on them).
- **Bash scripts:** Use `set -euo pipefail`. Mirror the bootstrap script's `pass`/`fail`/`info`/`warn`/`die` helpers and "Fix:" remediation block style. This is the same pattern `jam doctor` uses.
- **Dates in docs:** Use UTC, RFC 3339 where precise; use `YYYY-MM-DD` in headers. The spec currently dates to 2026-05-03.
- **Workspace layout:** Cargo workspace, Python package, and `package.json` live in this monorepo under `crates/`, `maestro/`, and `ui/` respectively. See `docs/layout.md` for the full directory tree.
- **Validation:** Run the relevant local validation after edits. For graph edits, run `tempyr validate`; for bootstrap scripts, preserve and exercise dry-run / verify-only paths where feasible.

## Tempyr Knowledge Graph

This repository uses Tempyr, a file-based knowledge graph for product and technical design.

### Graph Location

- Graph nodes: `graph/<type>/*.md` (Markdown files with YAML frontmatter, e.g. `graph/features/feat-session-replay.md`)
- Schema: `.tempyr/schema.toml`
- Config: `.tempyr/config.toml`
- Render templates: `.tempyr/render/`
- Interview sessions: `.tempyr/sessions/`

### Agent Workflow

- Prefer Tempyr MCP tools over direct graph file edits whenever possible.
- Use the interview flow for new features, epics, and larger graph expansions.
- Keep changes small and validate graph consistency after writing.
- Prefer updating existing nodes over creating near-duplicates.

### Tempyr Tools

When Tempyr MCP is available, prefer:

- `graph_search` / `graph_vsearch` / `graph_context`
- `graph_get_node`
- `graph_add_node` / `graph_add_edge`
- `graph_update_node`
- `graph_traverse`
- `graph_validate`
- `graph_render`
- `graph_ask`
- `interview_start` / `interview_answer` / `interview_commit`

### Graph Rules

1. Never rename node IDs manually. Use `tempyr rename`.
2. Use human-readable kebab-case slugs when creating node IDs manually.
3. Store edges bidirectionally in YAML frontmatter, and keep each edge list alphabetized by target.
4. Run `tempyr validate` after manual graph edits.
5. Prefer updating existing nodes over creating near-duplicates.
6. If a change affects retrieval quality, rebuild or update the index.

### Environment

- Embedding provider settings live in `.tempyr/config.toml`.
- API keys are typically loaded from Tempyr's shared Git-common-dir env (`tempyr/.env.local`), `.env.local`, `.env`, or the shell environment.
- Repo-local `.env.local` overrides shared worktree defaults when both are present.
- At each location, Tempyr loads `.env.local` before `.env`.
