---
id: risk-picker-going-off-rails
type: risk
status: identified
created: 2026-05-04T03:47:24.276462859Z
updated: 2026-05-04T03:47:24.276463445Z
---
**Threat #2 (medium likelihood, security-setup §1).** Picker writing garbage to its worktree, deleting files, running unexpected `cargo install` of dependencies.

Bounded by worktree creation protocol; minor recovery cost. Per-Picker worktree mode 700 prevents inter-Picker interference even though all Pickers share UID picker.