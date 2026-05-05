---
scope: blueberry/skill-packs
---

# Blueberry — Skill Pack Bridge

Blueberry has its own `.claude/skills/` directory with Claude-Code-level skill packs. These are NOT loaded by the Maestro — they're loaded by Pickers running as Claude Code in Blueberry's worktree. This skill explains the boundary.

<two_skill_layers>

**Maestro skills** (`/home/caleb/jamboree/skills/`):
- Loaded by the Maestro at session start via `read-skills(scope)`.
- Plain Markdown with `scope:` front-matter.
- Govern orchestration logic, dispatch decisions, project conventions, harness routing.
- Always-loaded: `Maestro.md`, `global.md`. Scope-matched: everything under `projects/`, `harnesses/`, `reviewers/`, `task-types/`, `agents/`.

**Blueberry skill packs** (`/home/caleb/blueberry/.claude/skills/<name>/SKILL.md`):
- Loaded by Pickers running as Claude Code (when working in Blueberry's worktree).
- Anthropic skill-pack format with `name` / `description` / `allowed-tools` front-matter.
- The `description` is critical — Claude Code uses it to auto-discover when to invoke the skill.
- Govern *how to do specific Blueberry work* (animation, SDF modeling, Tempyr ops).

These layers DON'T overlap and SHOULDN'T be merged.
</two_skill_layers>

<existing_blueberry_skill_packs>

| Skill pack | Triggers when | Source |
|---|---|---|
| `procedural-animation` | Task involves character animation, camera controllers, screen shake, FOV effects, springs, jiggle bones, IK, additive layers, lookAt, lean, impact effects, weapon sway, motion shaders, or "feel alive" work. | `/home/caleb/blueberry/.claude/skills/procedural-animation/SKILL.md` |
| `sdf-modeling` | Task is **art-directed visible SDF asset** (props, rocks, characters, cutters, analytic meshes for cel/outline). Excludes terrain algorithms. | `/home/caleb/blueberry/.claude/skills/sdf-modeling/SKILL.md` |
| `tempyr-interview` | Adding features, epics, requirements via guided interview. | `/home/caleb/blueberry/.claude/skills/tempyr-interview/SKILL.md` |
| `tempyr-ops` | Full MCP tool reference, dispatch commands, graph editing rules. | `/home/caleb/blueberry/.claude/skills/tempyr-ops/SKILL.md` |

Plus Blueberry agents in `/home/caleb/blueberry/.claude/agents/`:
- `anim-reviewer.md` — animation code review subagent.
- `sdf-reviewer.md` — SDF art asset review subagent.
- `tempyr-extractor.md` — extracts graph nodes from natural language (used by interview).
</existing_blueberry_skill_packs>

<how_pickers_use_them>
When the Maestro spawns a Claude Code Picker against Blueberry's worktree, Claude Code automatically:
1. Reads `.claude/settings.json` for hooks (`SessionStart` runs `tempyr journal bootstrap`).
2. Sees `.claude/skills/*/SKILL.md` and loads `description` fields into its tool-discovery layer.
3. Auto-invokes the relevant skill when a task description matches the description.

The Maestro doesn't need to manage this. The Maestro's job is to dispatch with the right `initial_prompt` so Claude Code's skill-discovery picks up the correct skill.

Example dispatch:
```
spawn-picker(spec={
    harness: "claude-code",
    initial_prompt: "Refactor the canyon generator to use spline-based seam protocols. ...",
    ...
})
```

Claude Code sees "canyon generator" / "spline" in the prompt; doesn't auto-invoke `sdf-modeling` (terrain code, not art assets); operates with normal code-editing tools.

Counterexample:
```
spawn-picker(spec={
    harness: "claude-code",
    initial_prompt: "Author a new visible rock prop for the playground scene as an SDF model. ...",
    ...
})
```

Claude Code sees "visible", "prop", "SDF model"; auto-invokes `sdf-modeling`; reads the principles doc.
</how_pickers_use_them>

<maestro_responsibility>
The Maestro's role:
1. Pick the right harness (Claude Code is the fit when Blueberry skill packs apply).
2. Compose the `initial_prompt` so it cleanly matches a skill pack's description.
3. Don't try to invoke Blueberry's skill packs from the Maestro side — they're not the Maestro's tool surface.

If a task could go either way (general code vs SDF-art), prefer the more specific framing (SDF-art) so the Picker invokes the relevant skill. This is more reliable than relying on the Picker's defaults.
</maestro_responsibility>

<adding_skill_packs>
If Blueberry adds a new `.claude/skills/<name>/SKILL.md`, no Maestro-side change is needed. Pickers pick it up automatically.

If you observe a recurring need that should become a Blueberry skill pack (specific workflow done multiple times across PRs), use `record-improvement-candidate(category="blueberry-skill-pack", description=..., motivation=...)` so the Manager can author one.
</adding_skill_packs>

<related>
- `harnesses/claude-code.md` — when to dispatch Claude Code.
- `projects/blueberry/sdf-art-policy.md` — SDF skill pack invocation rules.
- Blueberry's `CLAUDE.md` — full agent tooling reference.
</related>
