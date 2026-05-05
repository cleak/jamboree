---
id: comp-jam-skills-monorepo-dir
type: component
status: active
created: 2026-05-04T05:54:36.264144336Z
updated: 2026-05-04T05:56:38.864847146Z
edges:
- target: feat-self-improvement
  type: used_by
- target: task-write-initial-skills
  type: used_by
---
**The skills directory inside the Jamboree monorepo: `/home/caleb/jamboree/skills/`.** Per `dec-skills-in-monorepo-v1`.

Layout (initial v1 — Phase 1 minimum):
```
/home/caleb/jamboree/skills/
├── Maestro.md                              # System prompt (always loaded)
├── global.md                               # Cross-cutting guidance (always loaded)
├── projects/
│   └── blueberry/
│       ├── overview.md
│       ├── wslg-runtime.md
│       ├── brp-server.md
│       ├── journal-logging.md
│       ├── commit-validation.md
│       └── code-conventions.md
└── harnesses/
    └── codex-cli.md
```

Phase 2-5 additions: `reviewers/coderabbit.md`, `projects/blueberry/coderabbit-conventions.md`, `projects/blueberry/hot-paths.md`, `harnesses/{claude-code,opencode-deepseek}.md`, `task-types/*.md`, `agents/{patch-agent,research-completion-handler}.md`.

Permissions: mode 2770 caleb:maestro with setgid (so orchestrator-authored skills via `record-learning` get correct group ownership).

Runtime read path: TBD per `oq-runtime-skills-path`. For implementation Phase 1, recommend symlink `/home/maestro/.jam/skills/` → `/home/caleb/jamboree/skills/` to preserve hot-edit semantics.