---
id: task-secrets-bridge-via-sudo-pass
type: task
status: done
created: 2026-05-04T04:01:15.382725992Z
updated: 2026-05-06T09:12:51Z
---
For caleb-side CLI to read NATS tokens etc. — implement `sudo -n -u maestro -i pass show <key>` pattern (security-setup §6.2, §7.3 Option B).

Per `oq-cli-sudo-vs-duplicate-nats-token` — open question whether to do this on every CLI command or duplicate token in caleb's pass.

Implementation note (2026-05-06): `crates/jam-cli` now resolves NATS auth through a single helper. `NATS_TOKEN` takes precedence when set; otherwise the CLI attempts `sudo -n -u maestro -i pass show jam/nats/token` and uses the returned value. If the bridge is unavailable or the key is absent, the CLI falls back to unauthenticated local NATS so development stacks without NATS auth continue to work.

Verification (2026-05-06): unit coverage asserts the exact non-interactive sudo argv and rejects unsafe pass key paths. A live bridge probe ran with output redirected; sudo reached maestro's pass store, and the current machine reported `jam/nats/token` absent rather than a sudo/auth failure.
