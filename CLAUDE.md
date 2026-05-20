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

## GitHub App Auth: Installation Vs User-To-Server

`jam-svc-repo` uses two GitHub App auth modes for different paths (see `graph/decisions/dec-github-app-not-pat.md` and its 2026-05-11 addendum).

- **Installation token (server-to-server).** Used for read-heavy paths: PR-status polling, comment fetching, ETag-conditional reads. Gets the App's 15K/hour rate ceiling and per-installation isolation. Configured via `JAM_GITHUB_APP_ID` / `JAM_GITHUB_APP_INSTALLATION_ID` / `JAM_GITHUB_APP_PRIVATE_KEY` env vars or the matching `jam/pickers/github-app-*` pass keys.
- **User-to-server token (`ghu_*`).** Used for write paths: `gh pr create`, `git push`, PR-comment writes. Authorizes the App on behalf of a real user (`cleak`), so the resulting PRs land with `is_bot:false` and reviewer bots (CodeRabbit) auto-review through the normal path. Configured via `JAM_GITHUB_USER_TOKEN` or `jam/pickers/github-user-token` in maestro's pass store.

When the user token is missing, `jam-svc-repo` falls back to the installation token on the write path and posts `@coderabbitai full review` after PR open as a partial workaround, but CodeRabbit's incremental-skip then prevents further auto-reviews on subsequent pushes. The user token is the only complete fix.

### One-Time Setup

1. In the App settings (https://github.com/settings/apps/<your-app>):
   - Under **"Identifying and authorizing users"**, check **"Enable Device Flow"** (required for the headless auth helper).
   - Under **Optional Features**, click **"Opt-out"** next to **"User-to-server token expiration"**. Without this, the `ghu_*` token expires every 8 hours and jam-svc-repo silently falls back to installation-token + the comment-trigger workaround once it expires.
   - Save changes.
2. As `caleb`, run the device-flow helper:
   ```bash
   ./scripts/authorize-github-user-token.sh
   ```
   It prompts for the App's Client ID, opens a device-code flow, and pipes the resulting `ghu_*` token into maestro's pass at `jam/pickers/github-user-token`. It warns if a `refresh_token` comes back (meaning step 1 was not done).
3. Hot-patch `jam-svc-repo` to pick up the new token:
   ```bash
   jam deploy repo
   ```

Verify it worked: the next orchestrator-opened PR should show `author.is_bot:false` matching the authorizing user, and CodeRabbit should auto-review without any manual trigger.

## Deploying A Tool Service

`jam deploy <service>` is the one-shot path from a working-tree edit to a live service. It does three things:

1. `cargo build --release -p jam-svc-<service>` in this checkout.
2. Computes a version string (workspace `version` with `-<short-sha>-dirty` suffix if the tree is dirty; overridable with `--version`).
3. Publishes `patch.staged` over NATS pointing at `target/release/jam-svc-<service>`. `--from <path>` skips the build and uses a pre-built binary.

The `patch-agent` (process-compose service, runs as `maestro`) consumes `patch.staged` and runs §20.3:

- Copies the binary across the user boundary into `/home/maestro/.jam/bin/jam-svc-<service>-<version>`.
- Starts a candidate on a versioned subject prefix (`tool.<service>.v<version-slug>`).
- Gates on ping + smoke + `jam doctor` + recent-failed-events.
- Writes the new revision into the `routing-manifest` NATS KV bucket atomically.
- Drains the previous candidate.

The CLI waits for `patch.confirmed` or `patch.failed`. On failure it reports an incident under `/home/maestro/.jam/incidents/incident-<ULID>/` with `summary.json`, `health-post-apply.json`, `rollback-command.json`, `llm-diagnosis.json`, and the trailing 1000 journal events.

### Where Binaries Live

- Runtime, per-version patches: `/home/maestro/.jam/bin/jam-svc-<service>-<version>`. Written by patch-agent during `apply`. The routing manifest points at one of these.
- Canonical first-install location: `/opt/jam/bin/jam-<name>`. Written by `scripts/install-substrate.sh`. Used for fresh substrate brings, including for `patch-agent` itself.

Caleb has NOPASSWD sudo for installing into `/opt/jam/bin/` via the tightly-scoped `scripts/jam-install-bin` wrapper (installed to `/opt/jam/sbin/jam-install-bin` by `bootstrap-users.sh`, whitelisted in the generated sudoers as `Cmnd_Alias JAM_INSTALL_BIN`). The wrapper refuses sources outside `target/release/` and dest names that don't start with `jam-`.

### First-Patch Rollback Gotcha

If a post-apply check fails on the *first* patch ever applied to a service, the mechanical rollback fails with `current routing manifest has no previous_manifest_id` (there is no previous revision to revert to). The atomic swap has already happened by that point, so the new binary is live; only the rollback-on-failure path is broken. Treat reported failures on a first-deploy as "verify the new code is actually serving requests" (e.g. `strings $bin | grep <new-symbol>` and `jam health ping <service>`) before assuming the deploy did not land.

### Process-Compose Surfaces

Run as `maestro`:

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

**Coding agents (running as caleb) can use these NOPASSWD grants without prompting the human.** The full list (all in `/etc/sudoers.d/jam-users`, generated by `scripts/bootstrap-users.sh`):

| Grant | Who | Where used |
|---|---|---|
| `caleb -> maestro: ALL` | caleb | Anything maestro can do — read journals, restart processes via the unix socket, edit `~maestro/.jam/`, drive process-compose. |
| `caleb -> picker: ALL` | caleb | Inspect or clean per-task worktrees in `/home/picker/workers/`. |
| `maestro -> picker: ALL` | maestro | jam-svc-session spawns pickers as `picker` via `sudo -u picker codex …`. |
| `JAM_INSTALL_BIN` | caleb, maestro | `/opt/jam/sbin/jam-install-bin <src> <dest-name>` — installs binaries into `/opt/jam/bin/`. Source allowlist: `~caleb/jamboree/target/release/`, `~maestro/.jam/staging/`, `~maestro/.jam/bin/`. Dest must be `jam` or `jam-*`. Used by patch-agent's CanonicalBinary strategy. |
| `JAM_SERVICE_CONTROL` | caleb, maestro | `systemctl {restart,stop,start,status,is-active,is-enabled,reload,enable} jam.service` and `journalctl -u jam.service *`. The escape hatch when process-compose state corrupts and you need to bounce the substrate. |

Anything outside this list — general `sudo`, file ownership changes, package install, /etc edits, other systemd units — needs the human to run it interactively. Don't try to invoke them; surface the command for the human instead.

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
