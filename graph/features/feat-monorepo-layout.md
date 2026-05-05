---
id: feat-monorepo-layout
type: feature
status: active
created: 2026-05-04T03:28:23.832333013Z
updated: 2026-05-04T04:24:03.963545349Z
owner: caleb
edges:
- target: comp-monorepo-tree
  type: uses
- target: comp-source-vs-runtime-bridge
  type: uses
- target: jamboree-v5
  type: child_of
- target: principle-linux-only-deployment
  type: constrained_by
---
Jamboree is a **monorepo**. The spec, bootstrap scripts, runtime substrate (Rust crates), Maestro (Python), and UI (SolidJS) all live in the same git checkout at `/home/caleb/jamboree/` (layout.md).

Three premises typically argue for splitting spec from implementation; none hold here:
1. No audience separation (solo dev).
2. No independent versioning (spec is implementation-ready by design).
3. No security-context split at source level (runtime executes as `maestro`/`picker`, but source-of-truth is always at `~caleb/jamboree/`).

Top-level layout: `docs/`, `scripts/`, `crates/`, `maestro/` (Python), `ui/`, `Cargo.toml` workspace root.

Source vs. runtime bridge: built artifacts are staged from source, validated, and atomically swapped into `/home/maestro/.jam/` runtime via `jam patch apply` (§21.6). Source-of-truth never moves.

Reversibility: splitting later via `git filter-repo --subdirectory-filter`. Merging back is messier — so monorepo is the lower-risk default.