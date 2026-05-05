---
id: task-jam-setup-and-jam-doctor-13-checks
type: task
status: backlog
created: 2026-05-04T03:58:07.596350375Z
updated: 2026-05-04T04:09:12.202681639Z
edges:
- target: feat-jam-cli
  type: child_of
---
Phase 0 (§12). Implement `jam setup` and `jam doctor` with all 13 v5 checks (§11.4) plus the 11 multi-user additions from security-setup §10 = 24 total.

Per `comp-jam-setup`, `dec-13-check-setup-script`.

Acceptance: `jam setup` on fresh WSL → either passes all checks OR fails with specific actionable error per failed check. Each error names what's wrong, why it matters, and how to fix it.