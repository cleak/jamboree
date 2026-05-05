---
scope: task-types/ecs-refactor
---

# Task Type — ECS Refactor

Tasks that restructure Bevy ECS systems, components, resources, or plugins on Blueberry. Sub-class of compile-heavy-rust with tighter project-side discipline.

<concurrency_cap>
3 concurrent globally (shares the compile-heavy-rust cap per spec §6.7). One ECS refactor + 2 lighter compile-heavy tasks is a typical mix.
</concurrency_cap>

<harness_selection>
**Strongly prefer Claude Code** for ECS refactors:
- Bevy 0.18's ECS has subtle invariants (system ordering, Query types, Resources vs Components) where reasoning depth matters.
- Cross-system refactors benefit from extended thinking.
- Blueberry's `procedural-animation` skill pack (in `.claude/skills/`) auto-discovers when animation systems are touched — Claude Code picks this up automatically.

Codex CLI works for narrower refactors where the ECS structure isn't being rethought, just rewired.

OpenCode + DeepSeek works for overnight runs where the changes are mechanical (renaming, splitting plugins).
</harness_selection>

<bevy_018_invariants>
The Picker's prompt must call out these invariants explicitly so the model loads them into context:

- **Plan mode for changes spanning 3+ files** (per Blueberry's `CLAUDE.md`).
- **System scheduling**: procedural animation goes in `PostUpdate`, AFTER `AnimationPlugin` and physics. Chain order: deformation → skeletal additive → IK → camera.
- **Loose coupling via events**: emit "death" / "stage-changed" / etc. events rather than direct cross-system calls.
- **Compile errors over runtime errors**: prefer types that prevent the bug at compile time.
- **State machines need explicit `Completed` / `Consumed` terminal phases.** Without them, success-exit re-fires each tick and emits duplicate events.
- **`ObjectIdPlugin` shadow meshes**: never toggle `Visibility` on shadow mesh entities (PostUpdate sync overwrites). Always target source entity in `Update` or early `PostUpdate`.
- **Display scaling centralized in `src/display.rs`**: never patch `UiScale` or window descriptors directly.
- **`CollisionLayers` ↔ Rapier `CollisionGroups` desync is silent**: verify after any geometry/collider change.

Don't try to encode all of these in the Picker prompt — reference the relevant skills:
- `projects/blueberry/code-conventions.md` (full list).
- `projects/blueberry/blueberry-skill-pack-bridge.md` (Blueberry's own ECS skill packs).
</bevy_018_invariants>

<spawn_template>
```
spawn-picker(spec={
    task_id: "...",
    harness: "claude-code",  # default for ECS refactors
    sandbox_backend: "local" | "docker",
    sandbox_profile: "default",
    task_class: "ecs-refactor",
    initial_prompt: """
        ECS refactor: <description>

        Project context:
        - Blueberry / Bevy 0.18 voxel game.
        - This refactor touches: <crates / systems / plugins>.

        Apply Blueberry's ECS conventions (skills/projects/blueberry/code-conventions.md):
        - Loose coupling via events.
        - Component composition over inheritance.
        - State machines with explicit terminal phases.
        - Procedural animation in PostUpdate, after AnimationPlugin.
        - Don't toggle Visibility on shadow mesh entities.
        - Don't patch UiScale directly.

        For changes spanning 3+ files, use plan mode first.

        If touching animation: the procedural-animation skill applies (read docs/animation-principles.md).
        If touching SDF art: see skills/projects/blueberry/sdf-art-policy.md.

        Acceptance:
        - cargo fmt --check + cargo clippy --workspace --all-targets -- -D warnings.
        - cargo test --workspace.
        - Existing tests cover the refactored code.
        - Add regression tests for any bug fixed along the way.
        - PR opened ready-for-review (not draft).
    """,
    budget_usd: 10.00 - 30.00,
})
```
</spawn_template>

<related>
- `task-types/compile-heavy-rust.md` — parent task class.
- `task-types/risky-architecture.md` — when ECS refactor crosses into risky territory.
- `harnesses/claude-code.md` — preferred harness.
- `projects/blueberry/code-conventions.md` — full list of ECS invariants.
- `projects/blueberry/blueberry-skill-pack-bridge.md` — `procedural-animation` and other Blueberry skill packs.
</related>
