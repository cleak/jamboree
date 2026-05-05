---
id: risk-canonical-tempyr-worktree-corruption
type: risk
status: identified
created: 2026-05-04T03:47:20.703244958Z
updated: 2026-05-04T03:47:20.703245876Z
---
**§13.19 Canonical Tempyr worktree corruption (NEW v5).** The canonical worktree is long-lived; corruption (disk error, accidental rm -rf, bad rebase) is possible.

Mitigation: recovery path documented and tested (`jam tempyr canonical-worktree recreate` replays journal); `tempyr/tasks/` is journal-derived so rebuild is automatic; humans' `tempyr/nodes/` and `tempyr/specs/` are normal-committed-git, recoverable from origin.