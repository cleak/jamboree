---
id: comp-init-maestro-keyring-sh
type: component
status: planned
created: 2026-05-04T03:40:03.119357333Z
updated: 2026-05-04T05:04:45.891141698Z
edges:
- target: comp-multi-user-filesystem-layout
  type: depends_on
- target: comp-seed-maestro-secrets-sh
  type: depended_on_by
- target: constraint-wsl-pinentry-curses
  type: constrained_by
- target: dec-no-passphrase-on-maestro-gpg
  type: has_decision
- target: feat-multi-user-security-model
  type: used_by
---
`scripts/init-maestro-keyring.sh` — Maestro GPG + pass init (security-setup §5.1, §5.2). Creates an EDDSA ed25519 signing key + ECDH cv25519 encryption subkey under `maestro@localhost` with `%no-protection` (passphrase-less, recommended convenience-first choice).

Then `pass init maestro@localhost` creates `~maestro/.password-store/` with a `.gpg-id` pointing at the key.

If passphrase protection is desired: omit `%no-protection`, set up `gpg-agent` with cache TTLs, ensure `pinentry-curses` is installed (per `constraint-wsl-pinentry-curses`).

Already a script in repo per CLAUDE.md scripts/ list.