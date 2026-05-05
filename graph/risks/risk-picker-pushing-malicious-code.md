---
id: risk-picker-pushing-malicious-code
type: risk
status: identified
created: 2026-05-04T03:47:26.075756260Z
updated: 2026-05-04T03:47:26.075756836Z
---
**Threat #3 (medium likelihood, security-setup §1).** Picker pushing a hidden change to `main`, or to an unrelated repo it has push access to.

Doesn't require malice — induced by injection.

Mitigation: short-lived GitHub installation tokens (1h TTL) scoped to the App's installed repos. Picker doesn't get the GitHub App private key directly. Plus no `merge-pr` tool — direct merge to main requires human via GitHub UI.