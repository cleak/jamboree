---
id: comp-docker-sandbox-backend
type: component
status: active
created: 2026-05-04T03:39:24.201769451Z
updated: 2026-05-06T16:22:49Z
edges:
- target: comp-sandbox-backend-trait
  type: depends_on
- target: dec-hermes-as-three-subsystems
  type: has_decision
- target: feat-sandboxing-profile-x-backend
  type: used_by
- target: principle-adopt-subsystems-not-frameworks
  type: constrained_by
- target: principle-native-fs-only
  type: constrained_by
- target: principle-sandbox-blast-radius-not-behavior
  type: constrained_by
---
Linux container via Hermes' Docker backend (§6.2, §17.3). Hard FS / network isolation.

Vendored or wrapped from Hermes' Docker backend. Reimplemented in Rust if vendoring is messy; the design choices are what matter:
- Read-only repo bind-mount + read-write worktree mount.
- `--read-only` + `tmpfs:/tmp` for everything else.
- `--network=none` for hardened profile; `--network=bridge` with iptables rules for default.
- Env wipe + allowlist injection.

Default for unattended overnight runs (`default × docker`). Hardened-architecture task class uses `hardened × docker`.

`docker run --read-only --tmpfs /tmp -v <worktree>:/work:rw -v <bare-clone>:/repo.git:ro -w /work …` (§6.12).

Resource flags: `--cpus`, `--memory`, `--blkio-weight` (§6.4).

Boundary discipline (§17.3): we expose `SandboxBackend::Docker` from our own code; underlying flags happen to match Hermes' choices. If Hermes' Docker backend pivots, we don't have to follow.

Implementation note (2026-05-06): `crates/jam-svc-session` exposes the Docker backend through `spawn-picker`. The first implementation wraps the existing harness argv in `docker run --rm -i --init --read-only`, mounts the Picker worktree at `/work:rw`, mounts the worktree git common dir at `/repo.git:ro`, sets `--network=bridge` for `default` and `--network=none` for `hardened`, passes only the Picker env allowlist via `--env`, and labels containers with `org.jamboree.task` / `org.jamboree.session`. The smoke `scripts/smoke-docker-sandbox-backend.sh` proves the `default × docker` path with live NATS and an Alpine dry-run Picker, then proves the hardened network profile by attempting `wget http://example.org` and recording `network_blocked=1`. The companion benchmark `scripts/bench-docker-sandbox-compile.sh` uses the Blueberry repo and `blueberry-ops-base:latest` to keep compile-heavy overhead under 25%; the accepted run measured 7.4%.
