---
id: task-write-initial-skills
type: task
status: done
created: 2026-05-04T05:54:45.909136629Z
updated: 2026-05-04T16:08:36.507498447Z
edges:
- target: comp-jam-skills-monorepo-dir
  type: uses
- target: feat-self-improvement
  type: child_of
---
Write the initial skill files in `/home/caleb/jamboree/skills/` (per `comp-jam-skills-monorepo-dir`).

Phase 1 minimum (in_progress this session — drafted, awaiting review):
- `Maestro.md` — full system prompt (§8 skeleton + Blueberry-specific operating notes)
- `global.md` — minimal cross-cutting guidance
- `projects/blueberry/overview.md` — Blueberry context (Bevy 0.18 voxel game, where things live)
- `projects/blueberry/wslg-runtime.md` — WSLg env vars + decision rule
- `projects/blueberry/brp-server.md` — `.brp-port` discipline, BRP method list
- `projects/blueberry/journal-logging.md` — mandatory plan→outcome discipline
- `projects/blueberry/commit-validation.md` — cargo fmt/clippy/test gates
- `projects/blueberry/code-conventions.md` — Bevy ECS patterns, springs not lerp, etc.
- `harnesses/codex-cli.md` — initial routing affinity

Acceptance: each file has `scope:` front-matter; Maestro can call `read-skills(scope=blueberry/...)` and receive a coherent set; cross-references between files are valid.