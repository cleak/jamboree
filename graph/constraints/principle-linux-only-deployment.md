---
id: principle-linux-only-deployment
type: constraint
status: active
created: 2026-05-04T03:23:49.636368155Z
updated: 2026-05-04T04:31:34.820351508Z
edges:
- target: comp-pass-secret-backend
  type: constrains
- target: feat-monorepo-layout
  type: constrains
- target: feat-multi-user-security-model
  type: constrains
- target: feat-sandboxing-profile-x-backend
  type: constrains
---
Linux-only deployment is a non-goal exclusion (§1 Non-goals): no macOS, no native Windows. WSL is supported provided storage is on the native FS.

Implications:
- Sandbox backends (Local/Docker/SSH/Modal) all assume Linux primitives (cgroup v2, /proc, sudo, GPG).
- Cross-platform sandboxing is explicitly out of scope (§14).
- Future cross-platform work would need substantial sandbox-backend rework (§13.6).

Acceptable risk for Caleb's setup. Hardening to per-task isolation, network sandboxing, or systemd-managed services is additive and does not change the Linux-only premise.