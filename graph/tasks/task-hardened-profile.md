---
id: task-hardened-profile
type: task
status: done
created: 2026-05-04T03:59:29.578454877Z
updated: 2026-05-06T16:22:20.040174683Z
edges:
- target: feat-sandboxing-profile-x-backend
  type: child_of
---
Phase 4 (§12). Hardened profile: minimal HOME, restricted env, outbound allowlist.

Per `feat-sandboxing-profile-x-backend`.

Acceptance: Picker in `hardened × docker` cannot reach disallowed domains (verified by attempting `curl https://example.org` failing).

Implementation note (2026-05-06): the hardened Docker profile now maps to `docker run --network=none` with read-only rootfs, tmpfs `/tmp` and HOME, and only the Picker env allowlist. `scripts/smoke-docker-sandbox-backend.sh` spawns a `hardened × docker` dry-run Picker under live NATS and attempts `wget -q -T 3 http://example.org` from inside the container; the capture recorded `profile=hardened` and `network_blocked=1`.
