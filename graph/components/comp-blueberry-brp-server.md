---
id: comp-blueberry-brp-server
type: component
status: active
created: 2026-05-04T05:54:17.821412425Z
updated: 2026-05-04T05:56:01.399547060Z
edges:
- target: feat-picker-layer-three-tier
  type: used_by
---
**Blueberry's localhost-only Bevy Remote Protocol server** for inspecting/controlling the running game from outside (per Blueberry's `CLAUDE.md` Agent Tooling section).

Lives in Blueberry's `src/agent_tools/`. Pickers working on Blueberry use this for visual diagnostics rather than reading code blindly.

**Port discovery (worktree-aware):**
- BRP port derived from worktree path (range 15700-16699) — different worktrees never collide.
- Port written to `.brp-port` JSON at repo root: `{"address": "...", "port": ..., "pid": ...}`.
- `15702` is only the default for non-worktree runs.

**Pre-flight checklist for Pickers (mandatory):**
1. Check if `.brp-port` exists at repo root.
2. If exists, read it and check whether `pid` is still alive.
3. If PID alive, HTTP-probe the BRP endpoint to confirm responsive.
4. If alive AND responsive: **reuse the running instance** — do not launch another.
5. If alive but unresponsive: terminate ONLY the process matching the PID in `.brp-port`. Never terminate processes from other agents/worktrees.
6. If PID dead: file is stale; proceed with `cargo run`.

**Methods (selection from agent-tools.md):**
- `blueberry.scene.describe` — discover entity IDs (start here)
- `blueberry.scene.audit_transparency` — render route audit
- `blueberry.camera.get_pose` / `set_pose`
- `blueberry.debug.get` / `set` — render debug modes, physics, selection
- `blueberry.physics.raycast` — targeted physics probes
- `blueberry.script.enqueue` / `status` / `clear` — gameplay automation
- `blueberry.capture.request` / `status` — screenshot bundles

**Use BRP as the primary diagnostic for visual failures** (zero geometry, invisible entity, wrong render pass) BEFORE changing code.

Pickers running in Blueberry worktrees inherit this entire protocol — Jamboree doesn't need to reimplement port allocation, claim guards, or process discipline. Just respect the `.brp-port` workflow.