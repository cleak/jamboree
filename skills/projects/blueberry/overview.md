---
scope: blueberry
---

# Blueberry — Project Overview

A Bevy 0.18 playground for experimenting with cel shading, outline rendering, voxel terrain, and SDF art assets. Caleb's primary game project; Jamboree's initial target.

## Where things live

- **Repo root:** `/home/caleb/blueberry/` (caleb-owned, this is the pristine main checkout — orchestrator never writes here).
- **Canonical Tempyr worktree:** `/home/caleb/blueberry-jam/` (caleb:maestro mode 2770; orchestrator writes `graph/tasks/` here).
- **Picker worktrees:** `/home/picker/workers/<task-id>/` (per-task ephemeral; mode 700).
- **Trunk branch:** `main`.
- **Tempyr graph (in-repo):** `graph/` and config under `.tempyr/`. Pickers use `mcp__tempyr__*` tools and `tempyr` CLI from the worktree root.

## Key tech

- **Engine:** Bevy 0.18 (trust the codebase over web search — APIs differ from older docs).
- **Rust toolchain:** pinned in `rust-toolchain.toml`. Rust 1.94+.
- **Build cache:** `sccache` + `mold` linker, Mesa/RADV on iGPU. Per-Picker shared `target/` for compile-heavy task class.
- **Physics:** Rapier (with `CollisionLayers` game abstraction).
- **Rendering:** wgpu (Vulkan backend); cel shader + outline post-process; Moebius effects.
- **Audio:** authored under `src/audio/` with collision-driven impact routing.
- **Inventory:** runtime under `src/inventory/`.
- **SDF art assets:** signed distance field models with hard/organic operators and meshing recipes.

## Repo structure highlights

- `src/main.rs` — app wiring, plugins, demo scene setup.
- `src/lib.rs` — shared module exports.
- `src/agent_tools/` — BRP server, capture system, scripted automation, debug endpoints.
- `src/player/` — player controller, camera, input, debug.
- `src/moebius/` — Moebius post-process / render code.
- `src/sdf/` — SDF models, grid, meshing.
- `assets/shaders/*.wgsl` — shader assets (do not run `cargo fmt` on these).
- `crates/blueberry_jobs/`, `crates/blueberry_terrain_foundation/` — sub-crates (must pass workspace clippy).
- `docs/` — see `docs/README.md` for the documentation taxonomy.
- `journal/nightly/` — Python nightly analysis pipeline (separate from Tempyr's journal).
- `ops/` — Docker-based scheduled job framework (Ofelia).
- `graph/` and `.tempyr/` — Tempyr knowledge graph.

## How to run the game

The game is launched via WSLg with software Vulkan. See `projects/blueberry/wslg-runtime.md` for the full env block. Single-digit FPS by design — for screenshots and BRP smoke tests, not interactive play.

## How to inspect a running game

The localhost-only BRP server is the primary diagnostic tool. See `projects/blueberry/brp-server.md` for the `.brp-port` discovery protocol and method list. Use BRP for any silent visual failure (zero geometry, invisible entity, wrong render pass) BEFORE changing code.

## How to commit

See `projects/blueberry/commit-validation.md` for the mandatory pre-commit and pre-PR checks.

## Conventions adopted from Blueberry

Jamboree adopts Blueberry's existing conventions wherever possible (per `dec-adopt-blueberry-conventions`). Specifically:
- Tempyr usage patterns (in-repo graph, mandatory journal logging).
- Commit and PR validation gates.
- Conventional commit prefixes (`feat:`, `fix:`, `refactor:`, `bump:`, `tune:`).
- Bevy ECS coding style (see `projects/blueberry/code-conventions.md`).

If a project-side convention isn't documented in this directory, fall back to Blueberry's `CLAUDE.md` and `AGENTS.md` at the repo root — those are the project's source of truth and Jamboree shouldn't override them.
