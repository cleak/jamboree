---
id: task-vendor-hermes-docker-backend
type: task
status: done
created: 2026-05-04T03:59:27.576585242Z
updated: 2026-05-06T16:19:08.350735881Z
edges:
- target: feat-sandboxing-profile-x-backend
  type: child_of
---
Phase 4 (§12). Vendor or wrap Hermes Docker backend.

Per `comp-docker-sandbox-backend`, `dec-hermes-as-three-subsystems`.

Acceptance: Picker in `default × docker` runs in a container with read-only repo bind-mount + read-write worktree mount.

Implementation note (2026-05-06): `jam-svc-session` now accepts `sandbox_backend=docker` with `sandbox_profile=default|hardened` and wraps the Picker launch in `docker run` using the Hermes-shaped backend contract: `--read-only`, tmpfs `/tmp` and minimal HOME, worktree mounted read-write at `/work`, git metadata mounted read-only at `/repo.git`, traced env allowlist via `--env`, and container labels for task/session cleanup. `scripts/smoke-docker-sandbox-backend.sh` ran a live NATS + Docker `alpine:3.20` dry-run Picker; `spawn-picker` returned `sandbox_backend=docker`, the command executed inside `/work`, and the container wrote its capture file through the worktree mount.
