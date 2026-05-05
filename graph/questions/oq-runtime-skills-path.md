---
id: oq-runtime-skills-path
type: open_question
status: answered
created: 2026-05-04T05:54:08.289826003Z
updated: 2026-05-04T15:58:52.084135719Z
edges:
- target: dec-skills-direct-read-with-config
  type: answered_by
- target: feat-self-improvement
  type: question_for
---
**Where does the runtime read skills from?** Per `dec-skills-in-monorepo-v1`, source-of-truth is `/home/caleb/jamboree/skills/`. But the spec's source-vs-runtime separation (`comp-source-vs-runtime-bridge`) means the orchestrator runtime traditionally reads from `/home/maestro/.jam/skills/`.

Three candidates:
1. **Symlink** `/home/maestro/.jam/skills/` → `/home/caleb/jamboree/skills/`. Hot-edit works; runtime treats it normally.
2. **Direct read** from `/home/caleb/jamboree/skills/` with maestro group access (mode 2770 caleb:maestro). Couples runtime to source-of-truth path. No symlinks.
3. **Sync step** in `jam patch apply` that copies from monorepo to runtime. Hot-edit requires sync.

Option 1 or 2 preserves hot-edit; option 3 doesn't. Option 1 keeps runtime path consistent with the rest of `~maestro/.jam/`. Option 2 is simplest.

Decision deferred until first implementation phase.