---
id: task-nats-jetstream-up
type: task
status: done
created: 2026-05-04T03:58:02.357054855Z
updated: 2026-05-07T00:03:46Z
edges:
- target: feat-substrate-services
  type: child_of
---
Phase 0 (§12). NATS JetStream running under `process-compose`. Streams configured. KV buckets (`routing-manifest`, `harness-versions`, `dispatch-state`, `setup-result`, `patch-lock`) created.

Per `comp-nats-jetstream`, `dec-single-node-jetstream`.

Acceptance: smoke test publishes a fake `journal.test` event with a trace_id; verify it lands in the day's JSONL file with the trace_id field.

Implementation note (2026-05-06): Local process-compose smoke passed using `/tmp/jam-substrate/bin/process-compose`, temporary JetStream storage, and `target/debug/jam-nats-bridge`: `jam-nats-bridge` ensured the `journal` stream plus KV bucket streams including `KV_routing-manifest`, subscribed with its durable consumer, and a traced `journal.test` publish landed in `journal.test.jsonl` with the top-level `trace_id`. This is now rerunnable as `scripts/smoke-substrate-journal.sh`; the script starts an isolated process-compose project, verifies `journal` and `KV_routing-manifest` via JetStream API, publishes the traced test event, and checks the JSONL file. The real root-launched supervisor path is still unverified from this shell because `sudo -n` requires a password.

Follow-up note (2026-05-06): `scripts/smoke-substrate-journal.sh --existing`
now verifies an already-running production substrate by publishing the same
traced `journal.test` event to `NATS_URL` (default `nats://127.0.0.1:4222`) and
checking `/home/maestro/.jam/journal/<date>/journal.test.jsonl`. With current
machine state it fails loudly because production NATS is not reachable; the
isolated smoke path still passes.

Installer note (2026-05-06): `scripts/install-substrate.sh` now builds and
installs the first-party runtime binaries required by the current enabled
`process-compose.yaml` set (`jam`, `jam-nats-bridge`, `jam-svc-message`,
`jam-svc-supervise`, and `jam-ui-server`) in addition to the pinned
`nats-server` and `process-compose` binaries. Dry-run and verify-only modes can
run without root for preview/audit, while the real install still writes to
`/opt/jam/bin` from an interactive/root shell.

Verifier note (2026-05-06): `scripts/install-substrate.sh --verify-only` now
checks the pinned `nats-server` and `process-compose` versions, not only
executable bits, so installer verification matches the `jam doctor`
`substrate-binaries-installed` contract.

Smoke note (2026-05-06): `scripts/smoke-install-substrate.sh` now proves that
contract without root by staging cached `nats-server` / `process-compose` plus
release-built first-party runtime binaries into a temporary `INSTALL_DIR`, then
running `scripts/install-substrate.sh --verify-only` against it.

UI bundle note (2026-05-06): `scripts/install-substrate.sh` now also builds the
SolidJS UI and installs it to `/home/maestro/.jam/ui/dist` (or `UI_DIST_DIR`
for smoke/override), because enabled `jam-ui-server` fails loudly when its
static directory is missing. `scripts/smoke-install-substrate.sh` stages that
bundle in a temporary directory and verifies it with `--verify-only`.

Production-shaped smoke note (2026-05-06): without writing `/opt/jam/bin`,
`scripts/smoke-substrate-journal.sh --maestro-runtime` starts cached
`nats-server` and `target/debug/jam-nats-bridge` as the `maestro` user on
`nats://127.0.0.1:4222`, writes to `JAM_HOME=/home/maestro/.jam`, and cleans up
its temporary NATS store. The smoke passed and wrote trace
`01KQYS00000000000000000000` to
`/home/maestro/.jam/journal/2026-05-06/journal.test.jsonl`. This proves the
production journal path and multi-user permissions; the remaining production
blocker is installing/starting the pinned runtime under `/opt/jam/bin` from an
interactive/root shell.

Cleanup follow-up note (2026-05-06): the smoke cleanup path now terminates
`maestro` child processes under the `sudo -u maestro` wrappers before waiting
on the wrappers, so a failed runtime smoke does not hang or leave NATS/bridge
processes behind. After the fix, both the isolated smoke and
`--maestro-runtime` smoke passed; no `nats-server` / `jam-nats-bridge`
processes or port `4222` / `42241` listeners remained afterward.

Reverification note (2026-05-06): `scripts/smoke-install-substrate.sh` and
`scripts/smoke-substrate-journal.sh --maestro-runtime` both passed again after
the search-service smoke work. The former verified staged pinned substrate
binaries, enabled first-party service binaries, and the UI static bundle through
`install-substrate.sh --verify-only`; the latter wrote traced
`journal.test` to `/home/maestro/.jam/journal/2026-05-06/journal.test.jsonl`.

Local acceptance suite note (2026-05-06): `scripts/smoke-local-acceptance.sh`
now wraps the deterministic non-provider smokes for handoff. The core suite
passed with rootless substrate install verification, maestro-runtime journal
verification on port `42242`, message modes, research fake-provider flow,
search fake-backend routing/cooldown flow, and evolve coordinator dry-run. It
does not replace the production `--existing` substrate check after `/opt/jam/bin`
install.

Heavy suite note (2026-05-06): `scripts/smoke-local-acceptance.sh --heavy-only`
also passed. It verified atomic swap, Docker sandbox, cgroup resource limits,
Hermes evolution vendor dry-run/tests, and patch-agent recovery without leaving
NATS/tool-service listeners behind.

Summary artifact note (2026-05-06): a follow-up core suite rerun passed and
validated the retained machine-readable output at
`/tmp/jam-local-acceptance.5Irw7M/summary.json` plus per-smoke
`summary.jsonl` entries. A heavy-only rerun also passed with valid summary
output at `/tmp/jam-local-acceptance.qczeRk/summary.json` and left no NATS or
tool-service listeners behind.

External audit note (2026-05-06): `scripts/audit-external-acceptance.sh`
captures the production gap after local smokes: `/opt/jam/bin` binaries and
the UI bundle are not installed, production NATS is not reachable, and
`jam doctor` remains red until the interactive/root substrate install runs.
The latest audit retained JSON evidence at
`/tmp/jam-external-acceptance.SiziMK/summary.json`.

Manual evidence follow-up (2026-05-06): after adding explicit acceptance
evidence checks, the latest external audit retained
`/tmp/jam-external-acceptance.FVxNrV/summary.json`; production NATS remains
blocked on the interactive/root substrate install, separate from manual
deployment proof files.

Strict audit follow-up (2026-05-06): the latest external audit with strict JSON
evidence validation retained `/tmp/jam-external-acceptance.c2ZIh1/summary.json`.
Production NATS is still unreachable because `/opt/jam/bin` has not been
installed or started; a temporary valid-evidence override proved only the
manual-evidence branch and did not change the production substrate result.

Evidence-smoke follow-up (2026-05-07): the manual-evidence branch is now
covered by `scripts/smoke-external-audit-evidence.sh` and included in the core
local acceptance suite. It confirms evidence parsing and summary rows only; the
production `--existing` NATS and journal path remains blocked until the
interactive/root substrate install and start are completed.

Core suite rerun (2026-05-07): `scripts/smoke-local-acceptance.sh --core`
passed with the evidence smoke included and retained
`/tmp/jam-local-acceptance.5udTy1/summary.json`.
