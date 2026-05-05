---
id: dec-adopt-blueberry-conventions
type: decision
status: decided
created: 2026-05-04T05:53:59.579634513Z
updated: 2026-05-04T05:55:42.461317602Z
edges:
- target: feat-multi-user-security-model
  type: depended_on_by
- target: feat-picker-layer-three-tier
  type: depended_on_by
---
**Jamboree adopts Blueberry's existing conventions and flows wherever possible.**

Specifically:
1. **Tempyr usage patterns.** Blueberry already uses `mcp__tempyr__*` tools, in-repo graph, mandatory journal-logging discipline. Jamboree's Pickers should match this exactly — same tools, same patterns, same `plan/finding/dead_end/outcome` discipline.
2. **Commit validation gates.** Pickers run `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` before every commit; PR creation adds `cargo check --workspace` + `cargo test --workspace`.
3. **Conventional commit prefixes.** `feat:`, `fix:`, `refactor:`, `bump:`, `tune:`. Imperative, scoped summaries.
4. **WSLg runtime awareness.** Game runs use `LD_LIBRARY_PATH=/usr/lib/wsl/lib WGPU_BACKEND=vulkan WGPU_FORCE_FALLBACK_ADAPTER=1 BLUEBERRY_WINDOW_RES=1920x1080 cargo run --release` — single-digit FPS by design (software Vulkan). Used for screenshots/BRP smoke tests/visual diffs, NOT interactive play or perf profiling.
5. **BRP server discipline.** Localhost-only Bevy Remote Protocol server. Port discovery via `.brp-port` JSON at repo root (worktree-derived port range 15700-16699). Pre-flight checklist: read `.brp-port`, check PID liveness, HTTP-probe, reuse if alive+responsive, never kill processes from other agents.
6. **Multi-agent worktree isolation.** Blueberry already handles BRP port collisions across worktrees. Jamboree leverages this — multiple Pickers in different worktrees don't need separate orchestrator-side port allocation.
7. **Mandatory journal logging.** Every Picker session must log at least one `plan` and one final `outcome --final` to Tempyr's journal. Auto-emitted entries on task status transitions are NOT to be double-logged.
8. **Code conventions.** Bevy 0.18 (trust codebase over web search); critically damped springs, not lerp; ECS composition over inheritance; loose coupling via events; compile errors over runtime errors; explicit Completed/Consumed phases for one-time-signal state machines.

This is a strong constraint — when in doubt about what convention to apply, check Blueberry's `CLAUDE.md`/`AGENTS.md` first. Blueberry's existing patterns are the source of truth for project-side discipline.