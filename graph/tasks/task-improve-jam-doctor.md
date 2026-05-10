---
id: task-improve-jam-doctor
type: task
status: done
created: 2026-05-04T04:00:48.366038082Z
updated: 2026-05-06T20:53:07Z
---
Phase 9 (§12). Improve `jam doctor` based on real failures encountered during Phase 9 production hardening.

Implementation note (2026-05-06): added two Phase 9 learned checks to `jam-setup` / `jam doctor`: `root-sudo-noninteractive` warns when root-only bootstrap cannot run from the current noninteractive shell, and `substrate-binaries-installed` fails when pinned production substrate binaries (`/opt/jam/bin/nats-server` v2.11.0 and `/opt/jam/bin/process-compose` v1.40.1) are missing, non-executable, or version-drifted. This captures the two hardening failures hit during live smokes: missing NATS/process-compose on the production path and `sudo` requiring an interactive password.

Follow-up note (2026-05-06): all original deferred `jam doctor` placeholders now have concrete behavior. `clock-skew-vs-nats` requires loopback NATS and a successful NATS `INFO` probe, treating same-host clock as shared per `dec-single-node-jetstream`. `nats-server-reachable` validates `NATS_URL` by requiring a TCP connection and NATS `INFO` line. `github-app-key-valid` loads env / file / maestro pass GitHub App credentials and attempts the Octocrab installation-token exchange, warning only when no App config exists and failing on partial or invalid config. `harnesses-installed` checks concrete pins in `/home/maestro/.jam/config/projects/blueberry-harnesses.lock` or `JAM_HARNESS_LOCKFILE` against installed binary version and SHA-256, while honoring explicitly deferred lockfile entries. `maestro-pass-store-has-expected-keys` checks the recommended GitHub App / ntfy / Brave pass keys and points at `scripts/seed-maestro-secrets.sh` for remediation.

Follow-up note (2026-05-06): `canonical-tempyr-worktree-ownership` now performs the actual shared-permission audit for `/home/caleb/blueberry-jam`: root path owned by caleb, every non-symlink entry group-owned by maestro, and every directory has the setgid bit. `worktree-root-native-fs`, `pass-functional`, and `gpg-agent-running` now check the runtime owner (`picker` or `maestro`) rather than Caleb's shell-local permissions.

Follow-up note (2026-05-06): `substrate-binaries-installed` now also parses
`process-compose.yaml` and fails if any enabled first-party service command
under `/opt/jam/bin` is missing or non-executable. This catches the production
start failure where NATS/process-compose are installed but enabled services
such as `jam-nats-bridge`, `jam-svc-message`, `jam-svc-supervise`, or
`jam-ui-server` are not deployed yet.

Follow-up note (2026-05-06): the same check now includes enabled readiness
probe commands. This catches a missing `/opt/jam/bin/jam` before
`process-compose` starts enabled services whose health checks shell out to
`jam health ping ...`.

Follow-up note (2026-05-06): when `ui-server` is enabled, the check also
requires `/home/maestro/.jam/ui/dist/index.html`, matching `jam-ui-server`'s
startup requirement for the SolidJS static bundle.

Follow-up note (2026-05-06): the `substrate-binaries-installed` remediation
now includes `scripts/smoke-substrate-journal.sh --maestro-runtime` before
`--existing`, giving operators a rootless `maestro`-user NATS-to-JSONL proof
that does not require `/opt/jam/bin` to be installed yet.
