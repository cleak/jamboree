---
scope: blueberry/code
---

# Blueberry — Code Conventions

Patterns Pickers should follow when writing or modifying Blueberry code. Source: `/home/caleb/blueberry/CLAUDE.md` (Architecture, Coding Style, Animation & Game Feel, SDF Art Assets, Terrain Algorithm Gotchas sections).

## Bevy 0.18 specifics

- Bevy 0.18 APIs differ from older online docs. **Trust the codebase over web search results.**
- Don't run `cargo fmt` on `.wgsl` shader files.
- Display scaling is centralized in `src/display.rs`. Use `BLUEBERRY_UI_SCALE` (Bevy `UiScale`) and `BLUEBERRY_WINDOW_SCALE_FACTOR_OVERRIDE` (DPI). Do NOT patch `UiScale` or window descriptors directly.
- `SteamDeck=1` env var activates Steam Deck paths locally (BorderlessFullscreen, scale_factor_override=1.0, 1024px shadow map, Mesa/RADV) without hardware.

## ECS paradigm

- Use Bevy's Entity Component System architecture.
- Search the codebase for duplicates before adding new systems or components.
- Prefer **component composition** over inheritance-like patterns.
- Systems should be focused and single-purpose.
- **Components**: data structures attached to entities.
- **Systems**: functions that operate on entities with specific component combinations.
- **Resources**: shared global state.
- **Plugins**: group related systems and components.
- All common logic goes in a single library divided into hierarchical modules.

## Coupling and error handling

- **Prefer loose coupling through events.** Emit "death" events so other systems can react without direct coupling.
- **Compile errors are strongly preferred over runtime errors.** Verify as much as possible at compile time.
- **State machines that emit one-time signals must include explicit `Completed` / `Consumed` terminal phases.** Without it, the success-exit re-evaluates each tick and emits duplicate events.
- **`ObjectIdPlugin` shadow meshes** mirror source entity visibility through `PostUpdate` shadow sync. Never toggle `Visibility` on a shadow mesh entity — sync overwrites it each frame. Always target the source entity, in `Update` or early `PostUpdate`.

## Coding style

- Run `cargo fmt` — use `rustfmt` defaults (4-space indentation).
- `snake_case` for functions, modules, variables.
- `PascalCase` for structs, enums, traits.
- `SCREAMING_SNAKE_CASE` for constants.
- **No low-value comments.** Comments explain "why", not "what".
- Prefer self-documenting code through clear naming.
- Prefer explicit names over abbreviations (`damage` not `dmg`).

## Animation and game feel

- **ALWAYS use critically damped springs** instead of lerp for any smoothed/interpolated value (camera, rotation, UI, IK targets).
- **Never** use linear interpolation (`lerp(a, b, t * dt)`) for camera follow, character rotation, or any value that should ease in/out.
- For procedural animation principles, spring math, squash/stretch, jiggle bones, camera juice, and game feel systems: see `/home/caleb/blueberry/docs/animation-principles.md`.
- Procedural animation systems go in `PostUpdate`, AFTER `AnimationPlugin` and physics, in chain order: deformation → skeletal additive → IK → camera.

## SDF art assets

These rules are for **art-directed visible SDF assets only**, not terrain or world-generation algorithms. For terrain algorithms, the rules below in "Terrain Algorithm Gotchas" apply.

- Build SDF models from large readable primitives first; silhouette quality matters more than micro-detail.
- Design for cel-and-outline renderer: black-silhouette readability, primary→secondary→tertiary form hierarchy, broad planes over high-frequency detail.
- Match operator + meshing recipe to form language: hard booleans + `SdfMeshingRecipe::hard(...)` for crisp; `organic_*` + `SdfMeshingRecipe::organic()` for soft.
- **Hard subtract** on SDF geometry creates non-manifold edges at intersection rims. Prefer `organic_subtract` for any curved or organic host surface.
- For `organic_subtract`, blend radius < distance from cutter center to nearest host surface. Margin ≥ 1.5× blend radius — exceeding margin punches through.
- DC mesher `hermite_mu` must be ≥ `2 * cell_size` (e.g., `1.0` for a 0.5m grid). Weight `phi = (1 - |f|/mu)^2` silently returns zero geometry when `|f| >= mu` — mesher produces no vertices and no error.
- Features should span ≥ 2-3 grid cells with 1-2 cells of margin at the boundary.
- SDF primitive parameters are the **single source of truth** for both visual mesh and physics collider — derive collider dimensions and scale transforms from the same params. After geometry/scale changes, render the collider debug wireframe to confirm alignment.
- `CollisionLayers` (game abstraction) and Rapier `CollisionGroups` (physics engine) can diverge silently with no errors. When desynced, Rapier queries skip objects with no warning. Suspect desync first when a cast unexpectedly misses a prop with valid collider/position.
- For **art-directed visible SDF asset creation/review**, use the `sdf-modeling` skill before writing code.
- Don't use `sdf-modeling` for terrain algorithms, heightfields, voxel terrain chunks, procgen/noise systems, collision/pathfinding/spatial-query algorithms, renderer infrastructure, or general SDF algorithm work.
- For SDF art asset code review, use the `sdf-reviewer` subagent.

## Terrain algorithm gotchas

- **Multi-canyon terrain composition: apply MAX over all shoulder heights BEFORE applying MIN over all floor carves.** Reversing the order causes later floor carves to silently overwrite earlier shoulder heights.

## Workflow tips

- Use plan mode for changes spanning 3+ files.
- Put multi-session plans, audits, migration checklists in `docs/working/`.
- Use `docs/temp/` only for disposable scratch notes.
- **Never run git operations on the same worktree in parallel** — concurrent commands cause `index.lock` contention.

## When changing visuals — use BRP first

For any silent visual failure (zero geometry, invisible entity, wrong render pass), **use BRP as the primary diagnostic before changing code.** See `projects/blueberry/brp-server.md` for the protocol.

`blueberry.scene.describe` confirms entities exist. `blueberry.debug.get` inspects render debug state. `blueberry.scene.audit_transparency` reveals the live render route.

For transparency specifically, audit_transparency shows whether a prop is on `CelOpaque`, `TransparentPrimitiveRoot`, or a stray `TransparentFaceMesh` path.
