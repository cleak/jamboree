---
id: risk-malicious-package-install
type: risk
status: identified
created: 2026-05-04T03:47:33.352183403Z
updated: 2026-05-04T03:47:33.352184003Z
---
**Malicious package install** (security-setup §1).

If you `pip install` a package that exfiltrates data, no amount of user separation helps.

Out of scope for the multi-user model. Mitigation lives at the dependency-management layer (cargo deny, pip-audit).