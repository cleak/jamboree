---
id: principle-native-fs-only
type: constraint
status: active
created: 2026-05-04T03:23:49.455463942Z
updated: 2026-05-04T04:31:25.052028785Z
edges:
- target: comp-docker-sandbox-backend
  type: constrains
- target: comp-local-sandbox-backend
  type: constrains
- target: comp-modal-sandbox-backend
  type: constrains
- target: comp-multi-user-filesystem-layout
  type: constrains
- target: comp-ssh-sandbox-backend
  type: constrains
- target: feat-multi-user-security-model
  type: constrains
- target: feat-sandboxing-profile-x-backend
  type: constrains
- target: feat-tempyr-knowledge-and-journal
  type: constrains
- target: feat-tool-services-out-of-process
  type: constrains
---
**§2.14 Native filesystem only.**

All orchestrator data lives on a Linux native filesystem. Windows mounts (`/mnt/c/`, `/cygdrive/`) are explicitly refused. Setup scripts and runtime checks both verify and fail loudly if violated. Canonical implementation: `is_windows_mount` in §6.6.

*Why:*
- Git operations on Windows mounts are 10–100x slower (NTFS metadata round-trips).
- Linux file permissions don't apply on Windows mounts (`chmod 600` on `secrets.toml` is a lie).
- inotify watches don't propagate from Windows mounts reliably.

WSL is supported, but data must live on the WSL filesystem (`/home/<user>/`), not on `/mnt/c/`.

Path-validation is one of the named invariants in §6.6 (Invariant 4) and a `jam doctor` check.