---
id: dec-no-passphrase-on-maestro-gpg
type: decision
status: decided
created: 2026-05-04T03:46:49.866201331Z
updated: 2026-05-04T05:04:45.891142163Z
edges:
- target: comp-init-maestro-keyring-sh
  type: decision_for
---
**`%no-protection` (passphrase-less) GPG key for `maestro`** (security-setup §5.1).

Why: orchestrator runs unattended overnight; pinentry prompts on every secret access defeat the orchestration model. Key file is at `~maestro/.gnupg/private-keys-v1.d/`, mode 700, only `maestro` can read it.

If `maestro`'s home is compromised, the key is compromised — same risk profile as any application that holds a long-lived secret.

Passphrase protection is possible (omit `%no-protection`, configure `gpg-agent` cache TTLs, install `pinentry-curses` per `constraint-wsl-pinentry-curses`) but not the convenience-first default.