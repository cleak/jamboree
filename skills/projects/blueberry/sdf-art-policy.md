---
scope: blueberry/sdf
---

# Blueberry — SDF Art Asset Policy

When to invoke Blueberry's `sdf-modeling` skill (in Blueberry's `.claude/skills/sdf-modeling/`) vs handle SDF-related work as normal code.

<use_sdf_skill_when>
The task's deliverable is a **visible art asset** built as an SDF model:
- Stylized props (rocks, foliage, tools, set dressing).
- Character parts.
- Cutter volumes used for art-directed CSG.
- Analytic visual models for cel shading and outline rendering.

The skill covers: primitives, CSG (boolean ops), silhouette design, bounds, meshing recipes, normals, topology, and tests when they directly affect a visible asset.

The Blueberry skill expects the agent to read `docs/architecture/sdf-modeling-principles.md` first.
</use_sdf_skill_when>

<do_not_use_sdf_skill_for>
Even if the code mentions SDFs, CSG, grids, dual contouring, surface nets, voxels, or marching cubes, **don't** invoke the SDF skill for:
- Terrain generation or world-gen algorithms.
- Heightmaps and biome/noise systems.
- Voxel terrain chunk meshing.
- Collision/pathfinding/spatial-query algorithms.
- Renderer infrastructure.
- Imported mesh assets.
- Skeletal animation.
- 2D UI distance fields.
- General SDF algorithm work that doesn't produce a visible art asset.

For these: use normal code conventions (`projects/blueberry/code-conventions.md`).
</do_not_use_sdf_skill_for>

<dispatching_pickers>
When dispatching a Picker for SDF art work:
- `task_class`: `risky-architecture` if the asset is non-trivial; `light-edit` for small tweaks.
- `harness`: `claude-code` (best at the architecture-heavy framing required by the SDF principles doc); fallback `codex-cli`.
- `initial_prompt`: include "use the `sdf-modeling` skill" — Claude Code will load `.claude/skills/sdf-modeling/SKILL.md` automatically.
- For SDF art reviews, use the `sdf-reviewer` subagent (in Blueberry's `.claude/agents/sdf-reviewer.md`).

The `sdf-modeling` skill enforces critical invariants:
- DC mesher `hermite_mu >= 2 * cell_size` (silent zero-geometry failure mode otherwise).
- `organic_subtract` blend radius < distance to nearest host surface (punch-through otherwise).
- SDF primitive params are single source of truth for visual mesh AND physics collider.
- `CollisionLayers` ↔ Rapier `CollisionGroups` desync is silent — verify after geometry changes.

Don't try to encode these invariants here; the Blueberry skill is the source of truth.
</dispatching_pickers>

<terrain_caveat>
Terrain code touching SDFs (e.g. `crates/blueberry-terrain/src/canyon.rs` using SDF operators for world-gen) is **NOT** SDF art work — it's terrain algorithm work. Use `task-types/compile-heavy-rust.md` patterns and `projects/blueberry/code-conventions.md` for terrain-algorithm-specific gotchas (e.g. MAX-shoulder-before-MIN-floor ordering for multi-canyon composition).
</terrain_caveat>

<related>
- `projects/blueberry/blueberry-skill-pack-bridge.md` — how Blueberry's `.claude/skills/` integrate.
- `projects/blueberry/code-conventions.md` — terrain algorithm gotchas.
- `task-types/risky-architecture.md` — sandbox profile for non-trivial SDF art work.
</related>
