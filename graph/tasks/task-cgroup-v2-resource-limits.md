---
id: task-cgroup-v2-resource-limits
type: task
status: done
created: 2026-05-04T03:59:32.441806868Z
updated: 2026-05-06T16:33:17.515742987Z
edges:
- target: feat-sandboxing-profile-x-backend
  type: child_of
---
Phase 4 (§12). cgroup v2 resource limits for local-backend Pickers.

Per `comp-local-sandbox-backend`, §6.4.

Acceptance: CPU/memory caps enforced per task class; risky-architecture profile ionice class 3 (idle).

Implementation note (2026-05-06): local `spawn-picker` now wraps Pickers in user `systemd-run --scope` cgroups when `JAM_SESSION_USE_SYSTEMD_SCOPE=true` (default). Task-class limits are applied as `CPUQuota=800%` for compile-heavy/gameplay/ECS work, `CPUQuota=100%` plus `IOWeight=10` and `ionice -c 3` for `risky-architecture`, and `CPUQuota=200%` for other local tasks; memory is capped at `MemoryMax=8G`. `scripts/smoke-cgroup-resource-limits.sh` ran live NATS spawns and verified systemd properties (`8s` CPU quota + 8 GiB memory for compile-heavy, `1s` CPU quota + 8 GiB memory for risky) plus `ionice=idle` from inside the risky Picker.
