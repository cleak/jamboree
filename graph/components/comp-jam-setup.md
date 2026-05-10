---
id: comp-jam-setup
type: component
status: active
created: 2026-05-04T03:39:49.786056564Z
updated: 2026-05-06T20:29:22Z
edges:
- target: comp-multi-user-filesystem-layout
  type: depends_on
- target: comp-patch-agent
  type: depended_on_by
- target: dec-13-check-setup-script
  type: has_decision
- target: feat-jam-cli
  type: used_by
---
`jam setup` (and `jam doctor`) — preflight + ongoing health (§11.4). Refuses to install if environment isn't right. Every check has a specific error and a specific remediation hint per `principle-failure-surfaces-immediately`.

Full check list (13 from §11.4 + 11 multi-user additions from security-setup §10, plus Phase 9 learned failure checks):

1. Linux kernel (refuse non-Linux outright; WSL detected via `/proc/version`)
2. JAM_HOME on native FS
3. Worktree-root on native FS
4. Tempyr canonical worktree on native FS
5. `fs.inotify.max_user_watches >= 524288`
6. systemd available
7. NTP synced (`timedatectl show -p NTPSynchronized` returns `yes`)
8. Clock skew vs NATS server < 1s
9. `pass` functional for the `maestro` runtime user
10. `gpg-agent` responds for the `maestro` runtime user
11. NATS server reachable
12. Required harnesses installed at pinned versions (per `harnesses.lock`)
13. GitHub App key valid (test `octocrab` token exchange)
14. Service users `maestro` and `picker` exist with expected UIDs
15. Calling user is in `maestro` group (active in current shell)
16. `/etc/sudoers.d/jam-users` present and valid
17. `sudo -n -u maestro id` succeeds (NOPASSWD works)
18. `/etc/jam/bootstrap.log` present and matches expected version
19. JAM_HOME for current process is on native FS
20. Skills repo path exists and is readable by the running user
21. Canonical Tempyr worktree per active project has correct group ownership and setgid
22. maestro's pass store has the expected keys (per project config)
23. Picker spawn smoke test
24. picker cannot sudo (verify least privilege)
25. Root sudo is available non-interactively, or warns that root-only installers must run from an interactive/root shell
26. Pinned production substrate binaries exist and match `/opt/jam/bin/nats-server` v2.11.0 plus `/opt/jam/bin/process-compose` v1.40.1; enabled first-party process-compose service binaries under `/opt/jam/bin` exist and are executable

After setup succeeds, `setup-result.json` is written to NATS KV (`setup-result` bucket). Patch agent reads on first boot to know the verified-good baseline. Trace propagation health checks (`comp-trace-gap-detector`) are part of `jam doctor`.

Crate `crates/jam-setup/` provides `jam setup` and `jam doctor` binaries.

Implementation note (2026-05-06): `jam-setup` now uses the shared Rust `JAM_HOME` resolver from `jam-tools-core::paths`, so both JAM_HOME native-FS checks validate the path the current process actually resolves from security-setup §7.1.

Implementation note (2026-05-06): Phase 9 hardening added learned checks for the production substrate installer path: `root-sudo-noninteractive` surfaces noninteractive sudo limitations before agent-run bootstrap work stalls, and `substrate-binaries-installed` reports missing or drifted NATS/process-compose binaries with direct remediation through `scripts/install-substrate.sh`.

Implementation note (2026-05-06): `github-app-key-valid` is no longer a placeholder. `jam doctor` now reads GitHub App credentials from env or maestro pass keys (`jam/pickers/github-app-id`, `jam/pickers/github-app-installation-id`, `jam/pickers/github-app-key`) and attempts the Octocrab installation-token exchange. Missing credentials warn because App setup is external; partial, malformed, or rejected credentials fail loudly per `principle-failure-surfaces-immediately`.

Implementation note (2026-05-06): `nats-server-reachable` is no longer deferred. `jam doctor` parses `NATS_URL` (default `nats://127.0.0.1:4222`), opens a one-second TCP connection, and verifies the server sends a NATS `INFO` line. A stopped or wrong service now produces a required failure with a process-compose remediation.

Implementation note (2026-05-06): `maestro-pass-store-has-expected-keys` is no longer deferred. It checks the recommended maestro pass keys for GitHub App auth, ntfy, and Brave search, and warns with a direct `seed-maestro-secrets.sh` remediation when any are missing.

Implementation note (2026-05-06): `harnesses-installed` is no longer deferred. It reads the runtime Blueberry harness lockfile (`/home/maestro/.jam/config/projects/blueberry-harnesses.lock`, or `JAM_HARNESS_LOCKFILE`), ignores explicitly deferred pins, and fails if concrete pins do not match the installed harness version and SHA-256 checksum.

Implementation note (2026-05-06): `clock-skew-vs-nats` is no longer deferred. Because v5 uses single-node loopback NATS (`dec-single-node-jetstream`), the check requires a loopback `NATS_URL`, verifies the NATS `INFO` probe, and treats the clock as shared with the local host. Remote NATS authorities fail loudly instead of pretending skew is measurable.

Implementation note (2026-05-06): multi-user path checks now evaluate runtime-owned paths as the owning user when needed. `worktree-root-native-fs` resolves `/home/picker/workers` through `sudo -n -u picker realpath`, and `pass-functional` / `gpg-agent-running` check `maestro` rather than Caleb's shell-local stores. The systemd check accepts `/proc/1/comm == systemd` as a fallback for WSL environments where `/proc/1/exe` is not readable. `canonical-tempyr-worktree-ownership` now audits `/home/caleb/blueberry-jam` for caleb root ownership, maestro group ownership, and setgid directories instead of only checking existence.

Implementation note (2026-05-06): `substrate-binaries-installed` now parses
`process-compose.yaml` and verifies enabled first-party commands under
`/opt/jam/bin` exist and are executable. Disabled future services are ignored,
so the check tracks the actual supervisor enablement set instead of warning on
planned crates.

Installer alignment note (2026-05-06): `scripts/install-substrate.sh` now
installs the same enabled first-party binary set that this doctor check
requires. The script builds release artifacts as the checkout owner before
copying them into `/opt/jam/bin`, avoiding root-owned Cargo target churn.
Its `--verify-only` mode also checks pinned NATS/process-compose versions, so
the installer audit and doctor check fail on the same version-drift class.
`scripts/smoke-install-substrate.sh` covers the verifier path in a temporary
install dir without requiring root or touching production state.

Readiness-probe note (2026-05-06): the same check now scans enabled
`readiness_probe.exec.command` values, so support binaries such as
`/opt/jam/bin/jam` used by `jam health ping ...` probes are verified too.

UI asset note (2026-05-06): `scripts/install-substrate.sh --verify-only` now
checks the deployed UI bundle's `index.html` via `UI_DIST_DIR` (default
`/home/maestro/.jam/ui/dist`), matching `jam-ui-server`'s startup requirement.
`jam doctor` also checks `/home/maestro/.jam/ui/dist/index.html` when
`ui-server` is enabled in `process-compose.yaml`.

Remediation note (2026-05-06): `substrate-binaries-installed` now recommends
`scripts/smoke-substrate-journal.sh --maestro-runtime` as the rootless proof
between installer verification and the final production `--existing` smoke.
