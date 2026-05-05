---
id: constraint-wsl-pinentry-curses
type: constraint
status: active
created: 2026-05-04T03:23:50.876193180Z
updated: 2026-05-04T04:31:14.647238598Z
edges:
- target: comp-init-maestro-keyring-sh
  type: constrains
- target: feat-tech-stack-hardening
  type: constrains
---
WSL has no GUI pinentry by default. To use a passphrase-protected GPG key (the non-default convenience choice for `maestro`), `pinentry-curses` must be installed and `~/.gnupg/gpg-agent.conf` must point at it (§11.3.4 + security-setup §9.6).

The recommended convenience-first choice is `%no-protection` (passphrase-less key) for maestro per security-setup §5.1 — but if a passphrase is required, this constraint applies.