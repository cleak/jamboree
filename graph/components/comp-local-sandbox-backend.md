---
id: comp-local-sandbox-backend
type: component
status: planned
created: 2026-05-04T03:39:23.142462730Z
updated: 2026-05-04T04:44:20.534198325Z
edges:
- target: comp-sandbox-backend-trait
  type: depends_on
- target: feat-sandboxing-profile-x-backend
  type: used_by
- target: principle-native-fs-only
  type: constrained_by
- target: principle-sandbox-blast-radius-not-behavior
  type: constrained_by
---
Same machine, native process. Fast (no container overhead), shared build cache (§6.2). Worktree-only guarantee is **soft** — path-prefix invariant in tools is the only check; `worktree-diff` and `find-conflicts` ignore anything outside the worktree (§6.12).

For `local`, network is unrestricted by default. The `hardened-local` profile adds a process-level outbound-allowlist via a small forward-proxy that drops disallowed domains. The allowlist defaults to: harness API endpoints, GitHub, crates.io, npmjs.com, pypi.org. Project-config can extend per-project (§6.3).

Resource limits via cgroup v2 (§6.4): CPU configurable per task class (compile-heavy Rust up to 8 cores; review tasks 2). Memory 8 GiB default cap. I/O ionice class 2 default; risky-architecture profile uses class 3 (idle).