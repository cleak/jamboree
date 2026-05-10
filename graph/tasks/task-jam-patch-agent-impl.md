---
id: task-jam-patch-agent-impl
type: task
status: done
created: 2026-05-04T04:00:18.919444693Z
updated: 2026-05-06T15:35:16.955737190Z
edges:
- target: feat-hot-patching
  type: child_of
---
Phase 7 (§12). `jam-patch-agent` with pinned dependencies, focused LLM client.

Per `comp-patch-agent`, `dec-patch-agent-deterministic-then-llm`, `metric-patch-agent-llm-budget`.

Acceptance: apply a deliberately-broken patch; verify deterministic health checks catch it within 30s and trigger mechanical rollback. Apply a broken patch that mechanical rollback can't fix; verify LLM diagnosis runs, attempts, fails, ntfy human with incident dump.

Implementation note (2026-05-06): added the `jam-patch-agent` Rust crate and live recovery smoke. The agent handles `patch.applied`, confirms healthy patches, catches ping/smoke/doctor/recent-failure deterministic failures, retries rollback across the patch-lock release race, relaunches drained rollback routes, emits `patch.rolled-back-successfully` on successful recovery, and escalates unrecoverable patches through the LLM command hook, incident dump, `patch.failed`, critical `notify.human`, and dispatch pause.

Verification (2026-05-06): `scripts/smoke-patch-agent-recovery.sh` stages real `jam-svc-observe` binaries in a temporary `$JAM_HOME`, uses `JAM_OBSERVE_LIST_BLOCKERS_BROKEN=true` to make `ping` pass while the known-safe smoke returns the wrong shape, proves mechanical rollback restores `tool.observe.v009`, then makes the rollback route unhealthy and verifies `/bin/false` is attempted as the LLM hook, `llm-diagnosis.json` records `attempted=true` / `status=exit-1`, and `patch.failed` plus critical `notify.human` are published with the incident directory.
