# Jamboree — Repository Layout

**Status:** Decision record
**Decided:** 2026-05-03
**Supersedes:** earlier CLAUDE.md guidance that called for a separate implementation repo.

---

## Decision

Jamboree is a **monorepo**. The spec, bootstrap scripts, runtime substrate (Rust crates), Maestro (Python), and UI (SolidJS) all live in the same git checkout at `/home/caleb/jamboree/`.

## Why monorepo

Three premises typically argue for splitting spec from implementation. None hold here:

1. **No audience separation.** Solo dev; no readers want the spec without the source.
2. **No independent versioning.** The spec is implementation-ready by design; a spec change usually implies an impl change and vice versa. A two-repo split would require two coordinated PRs for what is logically one change.
3. **No security-context split at the source level.** Runtime executes as `maestro` and `picker` — but that is a *deploy* concern. Source-of-truth always lives at `~caleb/jamboree/` regardless of where it runs.

The opposite default (separate impl repo) buys nothing for our situation, and adds synchronization tax.

## Source vs. runtime

Two distinct concepts. Don't conflate them.

| | Lives at | Owned by | Purpose |
|---|---|---|---|
| **Source** | `/home/caleb/jamboree/` | `caleb` | Editable, version-controlled, the only thing humans modify |
| **Runtime** | `/home/maestro/.jam/` | `maestro` | What the Maestro process actually executes against |

The bridge between them is `jam patch apply` (per spec §21.6) — a hot-patch flow where built artifacts are staged from source, validated, and atomically swapped into the runtime location. Until that flow exists (Phase 3+), runtime layout is documented in `security-setup.md` §7.

## Top-level layout

```
/home/caleb/jamboree/
├── CLAUDE.md                      # Agent guidance for this repo
├── README.md                      # (TBD, future)
│
├── docs/                          # Specs and decision records
│   ├── proposal-v5.md             # Architecture spec (§0–§24)
│   ├── security-setup.md          # Multi-user isolation addendum
│   └── layout.md                  # This file
│
├── scripts/                       # System bootstrap and ops scripts
│   ├── bootstrap-users.sh         # Creates maestro/picker users + sudoers
│   ├── install-cli-tools.sh       # Per-user codex + claude-code installs
│   ├── cli-tools-update.sh        # Daily auto-update (invoked by cron)
│   ├── init-maestro-keyring.sh    # Maestro GPG + pass init
│   └── seed-maestro-secrets.sh    # Interactive secrets walk
│
├── crates/                        # Rust workspace (Phase 3+)
│   ├── jam-cli/                   # The `jam` CLI binary
│   ├── jam-svc-observe/           # Observation tool service (§4.2)
│   ├── jam-svc-supervise/         # Picker supervisor (§4.4.8)
│   ├── jam-stall-detector/        # Stall detector (§4.4.6)
│   ├── jam-ui-server/             # UI HTTP/WS server (§4.11)
│   └── …                          # See spec §4 for the full set
│
├── maestro/                       # Python Maestro package (Phase 3+)
│   ├── pyproject.toml
│   └── src/jam_maestro/           # Package source
│
├── ui/                            # SolidJS UI (Phase 3+)
│   ├── package.json
│   └── src/
│
└── Cargo.toml                     # Rust workspace root (Phase 3+)
```

The Rust workspace, Python package, and UI directories don't exist yet — they get scaffolded when Phase 3 begins. The decision to put them here rather than in a separate repo is the substance of this document.

## Naming inside the monorepo

- **Lowercase** for code identifiers and paths: `jam_maestro` (Python package), `maestro/` (dir), `maestro.toml` (config), `crates/jam-svc-*/`.
- **Capitalized** for prose references to roles: **the Maestro**, **the Pickers**, **the Manager**.
- The Linux usernames `maestro` and `picker` are lowercase by convention; this matches the source-dir naming.

There is no separate "lowercase technical-role" spelling for the Maestro or Pickers — earlier drafts had that distinction (with lowercase "conductor" / "workers"); we've dropped it. See CLAUDE.md naming section.

## When to revisit this decision

The monorepo is the right call for solo-dev-on-one-codebase. Concrete triggers that would justify revisiting:

- The spec becomes a public reference document (e.g., users want to read it without seeing source).
- A second implementation in a different language emerges (multi-language ports of the same spec).
- The implementation grows enough that build/CI on the whole repo gets slow (>5 min for a no-op).
- Multiple contributors with different access needs to spec vs. impl.

Until one of those is true, monorepo wins.

## Reversibility

Splitting later is cheap: `git filter-repo --subdirectory-filter docs/` extracts a subtree with full history. Merging two repos back together is messier (preserves history but with friction). So if there's any doubt, monorepo is the lower-risk default — start unified, split later if needed.
