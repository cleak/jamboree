---
id: risk-kernel-level-escapes-wsl
type: risk
status: accepted
created: 2026-05-04T03:47:31.507279812Z
updated: 2026-05-04T03:47:31.507280381Z
---
**Kernel-level container/sandbox escapes** (security-setup §1).

WSL shares a kernel with Windows; a kernel-level container escape would bypass user separation. Acceptable risk for solo dev workstation.