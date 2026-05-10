---
id: feat-sandboxing-profile-x-backend
type: feature
status: draft
created: 2026-05-04T03:28:21.197781746Z
updated: 2026-05-06T16:33:35Z
owner: caleb
edges:
- target: api-sandbox-backend-contract
  type: exposes
- target: api-worktree-create-protocol
  type: exposes
- target: comp-docker-sandbox-backend
  type: uses
- target: comp-local-sandbox-backend
  type: uses
- target: comp-modal-sandbox-backend
  type: uses
- target: comp-sandbox-backend-trait
  type: uses
- target: comp-ssh-sandbox-backend
  type: uses
- target: comp-untrusted-string-newtype
  type: uses
- target: comp-workspace-key-newtype
  type: uses
- target: comp-worktree-create-protocol
  type: uses
- target: jamboree-v5
  type: child_of
- target: principle-linux-only-deployment
  type: constrained_by
- target: principle-native-fs-only
  type: constrained_by
- target: principle-provider-agnostic-everywhere
  type: constrained_by
- target: principle-sandbox-blast-radius-not-behavior
  type: constrained_by
- target: task-cgroup-v2-resource-limits
  type: parent_of
- target: task-hard-fs-network-isolation-tests
  type: parent_of
- target: task-hardened-profile
  type: parent_of
- target: task-jam-svc-worktree-creation-protocol
  type: parent_of
- target: task-vendor-hermes-docker-backend
  type: parent_of
---
Picker sandboxing is **profile × backend** (§6.2):

- **Profile** (what the Picker can do): `default` | `hardened`.
- **Backend** (where the Picker runs): `local` | `docker` | `ssh` | `modal`.

Combinations target task class. `local × default` for fast dev iteration (soft enforcement). `default × docker` is the default for unattended overnight runs. `hardened × docker` for risky-architecture tasks. `hardened × modal` for elastic burst.

Network sandboxing comes from Docker/SSH/Modal backends (§6.3); for `local`, network is unrestricted by default; `hardened-local` adds a process-level outbound-allowlist via a small forward-proxy.

Resource limits via cgroup v2 for local (§6.4): CPU/memory/I/O per task class.

Build cache strategy (§6.5): shared `target/` + sccache + Mold linker for Bevy compile times.

Path safety invariants (§6.6) + concurrency caps (§6.7).

Implementation note (2026-05-06): the Docker backend path has landed for `default × docker` and `hardened × docker`, and local-backend Pickers now receive cgroup v2 resource scopes. The live smokes prove default container launch, hardened outbound blocking, task-class CPU/memory/I/O limits, and hard FS/network isolation. The compile-heavy Docker benchmark uses `/home/caleb/blueberry` with `blueberry-ops-base:latest` and measured 7.4% overhead against the 25% acceptance threshold.
