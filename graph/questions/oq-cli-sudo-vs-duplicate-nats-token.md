---
id: oq-cli-sudo-vs-duplicate-nats-token
type: open_question
status: open
created: 2026-05-04T03:47:46.394985527Z
updated: 2026-05-04T03:47:46.394986042Z
---
**Should the `jam` CLI shell out to sudo on every command, or duplicate the NATS token in caleb's pass?** (security-setup §6.2)

Option A (default): `sudo -n -u maestro -i pass show jam/nats/token` works without password thanks to sudoers rule. Clean (single source of truth) but adds a sudo round-trip per CLI command.

Option B: `pass insert jam/nats/token` as caleb (separate from maestro's). Less clean (duplicate secret) but avoids sudo round-trip.

TBD based on operational experience.