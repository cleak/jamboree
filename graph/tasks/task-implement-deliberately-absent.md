---
id: task-implement-deliberately-absent
type: task
status: backlog
created: 2026-05-04T04:01:46.383034088Z
updated: 2026-05-04T04:01:46.383034965Z
---
Audit Maestro tool surface for **deliberately absent** tools (§5.9). These must NOT exist:
- `read-file`, `write-file`, `run-command` (Pickers do file ops, not Maestro)
- `merge-pr` (only `request-human-merge`)
- `add-tool` at runtime (use `propose-tool-change`)
- `eval`, `exec`, `python -c` (banned at lint level)
- `set-task-plan-note` (task plans are session-scoped)
- `auto-rebase`, `auto-merge`, `auto-update-tempyr-node`
- `fork-Maestro`, `clone-session`

Per `principle-structure-in-tools-not-policy`, `insight-no-tool-no-possibility`.

Acceptance: code review confirms none of these exist in the tool registry; tests verify by name that calling them produces "no such tool" errors.