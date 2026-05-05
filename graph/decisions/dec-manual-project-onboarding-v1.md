---
id: dec-manual-project-onboarding-v1
type: decision
status: decided
created: 2026-05-04T05:38:08.581684555Z
updated: 2026-05-04T05:39:23.128963892Z
edges:
- target: feat-jam-cli
  type: depended_on_by
- target: task-document-blueberry-manual-onboarding
  type: blocks
---
**For v1: Blueberry project onboarding is manual and documented; `jam project add` is deferred.**

Day-one workflow (manual):
1. `bootstrap-users.sh` runs (creates users, sudoers, jam-skills dir).
2. Operator manually creates `~/.jam/config/projects/blueberry.toml` (template documented).
3. Operator manually creates `~/.jam/config/projects/blueberry-harnesses.lock` from a documented template.
4. `jam tempyr canonical-worktree create blueberry` (or manual `git worktree add`) creates `~/code/blueberry-tempyr-live/`.
5. Skills repo seed (Maestro.md + minimal scaffolding) populated from a seed location.

`jam project add <name>` is a future CLI feature (post-v1) that automates these steps. Captured as `task-jam-project-add-future`.

Why manual now: only one project exists; automating an N=1 case is over-engineering. Documenting the steps is enough.