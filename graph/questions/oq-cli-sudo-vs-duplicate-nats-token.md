---
id: oq-cli-sudo-vs-duplicate-nats-token
type: open_question
status: answered
created: 2026-05-04T03:47:46.394985527Z
updated: 2026-05-06T09:12:51Z
---
**Should the `jam` CLI shell out to sudo on every command, or duplicate the NATS token in caleb's pass?** (security-setup §6.2)

Option A (default): `sudo -n -u maestro -i pass show jam/nats/token` works without password thanks to sudoers rule. Clean (single source of truth) but adds a sudo round-trip per CLI command.

Option B: `pass insert jam/nats/token` as caleb (separate from maestro's). Less clean (duplicate secret) but avoids sudo round-trip.

Answer (2026-05-06): use Option A by default. `jam` reads `NATS_TOKEN` first for explicit overrides, then bridges to maestro's pass store with `sudo -n -u maestro -i pass show jam/nats/token`, then falls back to no token for unauthenticated local development NATS. Do not duplicate the token into caleb's pass unless operational experience shows the sudo round-trip is a real bottleneck.
