---
id: risk-shell-access-as-caleb
type: risk
status: accepted
created: 2026-05-04T03:47:29.691285275Z
updated: 2026-05-04T03:47:29.691285769Z
---
**Compromise of caleb's shell bypasses multi-user defenses** (security-setup §1).

NOPASSWD sudo to `maestro` means anyone in caleb's terminal can become `maestro`. That's fine — if they have your shell, you have bigger problems.

Acceptable risk for solo dev workstation. Tightening to require a passphrase would mean typing it many times per day.