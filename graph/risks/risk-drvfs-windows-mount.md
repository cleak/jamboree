---
id: risk-drvfs-windows-mount
type: risk
status: identified
created: 2026-05-04T03:47:35.208928122Z
updated: 2026-05-04T03:47:35.208928939Z
---
**WSL drvfs interaction** (security-setup §9.7).

Windows drives mount at `/mnt/c/`, `/mnt/d/`, etc. Case-insensitive, different permission semantics, slow for git operations. Bootstrap script and `jam doctor` refuse to install if orchestrator paths resolve to drvfs (per `principle-native-fs-only`).

Mitigation: always keep orchestrator state on Linux native filesystem.