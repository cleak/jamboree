---
scope: task-types/shader-variant
---

# Task Type — Shader Variant

Tasks that author or modify WGSL shaders under `/home/caleb/blueberry/assets/shaders/` (or shader-related Rust glue).

<concurrency_cap>
8 concurrent globally (shares the light-edit / doc-generation slot per spec §6.7). Shader work compiles fast (no Bevy recompile needed unless types change).
</concurrency_cap>

<sandbox_profile>
`default × local`. Shaders are hot-reloadable; iteration is fast.

For risky shader work (new render pass, change to a depth or transparency pipeline), upgrade to `task-types/risky-architecture.md`.
</sandbox_profile>

<harness_selection>
**Claude Code** is the typical default — shader work benefits from reasoning about visual outcomes and matrix math correctness.

**Codex CLI** for narrow shader edits where the math is already correct and you're just wiring uniforms.

Avoid OpenCode + DeepSeek for shader work — DeepSeek's reasoning slows down what should be a tight iteration loop, and there's no compile-time saving since shader compile is fast.
</harness_selection>

<wgsl_specifics>
- **Don't run `cargo fmt` on `.wgsl` files** — they're not Rust.
- WGSL has its own validation; Bevy's shader pipeline catches errors at load time.
- Hot-reload: shaders reload without restarting the game (in Bevy 0.18). Iteration cycle is sub-second.

Verify shader changes via BRP screenshot capture per `projects/blueberry/brp-server.md`:
1. Apply shader change.
2. Game hot-reloads.
3. `blueberry.capture.request` for the relevant scene.
4. Diff screenshot against baseline.
</wgsl_specifics>

<related_skills>
For shader work touching:
- **Cel shading / outline rendering**: that's the project's primary visual style. Trust the existing pipeline; don't redesign.
- **Moebius post-process** (`src/moebius/`): hot path; verify no per-pixel cost regression.
- **Animation/motion shaders**: invoke Blueberry's `procedural-animation` skill pack (auto-discovered by Claude Code).
- **SDF / cutter material**: invoke `sdf-modeling` skill pack only if the work is a visible art asset, not infrastructure.
</related_skills>

<spawn_template>
```
spawn-picker(spec={
    task_id: "...",
    harness: "claude-code",
    sandbox_backend: "local",
    sandbox_profile: "default",
    task_class: "shader-variant",
    initial_prompt: """
        Shader variant: <description>

        Project: Blueberry / Bevy 0.18 / wgpu / Vulkan backend.
        Target shader: assets/shaders/<file>.wgsl.

        Conventions:
        - WGSL only; don't run cargo fmt on .wgsl files.
        - Hot-reload works; iterate via BRP screenshot capture (skills/projects/blueberry/brp-server.md).
        - For visual diffs: capture before and after with the same scene.

        Acceptance:
        - Shader compiles (Bevy reports no errors at load).
        - Visual outcome matches description (verify via BRP screenshot).
        - cargo check passes if Rust glue changed.
        - cargo fmt --check + cargo clippy on Rust changes.
        - PR opened ready-for-review with before/after screenshots in the body.
    """,
    budget_usd: 2.00 - 8.00,
})
```
</spawn_template>

<related>
- `harnesses/claude-code.md` — preferred harness.
- `projects/blueberry/brp-server.md` — screenshot capture protocol.
- `projects/blueberry/code-conventions.md` — shader-related conventions.
- `task-types/risky-architecture.md` — when shader work crosses into pipeline-changing territory.
</related>
