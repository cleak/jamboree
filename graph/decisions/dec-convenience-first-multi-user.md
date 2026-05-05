---
id: dec-convenience-first-multi-user
type: decision
status: decided
created: 2026-05-04T03:46:46.629690556Z
updated: 2026-05-04T05:04:36.355579855Z
edges:
- target: comp-sudoers-jam-users
  type: decision_for
---
**Convenience-first multi-user model: NOPASSWD sudo, no per-task user creation, no Docker required** (security-setup §0).

Defends substantially against threat #1 (prompt-injection-driven exfiltration) and partially against #2/#3 at low operational cost. Does NOT defend against an attacker who already has caleb's shell.

Hardening to per-task user model is possible later by enabling Docker backend (§6.2) without changing this baseline.