---
id: task-atomic-swap-procedure-impl
type: task
status: done
created: 2026-05-04T04:00:15.860123654Z
updated: 2026-05-06T15:12:14.888469479Z
edges:
- target: feat-hot-patching
  type: child_of
---
Phase 7 (§12). Atomic-swap procedure for tool services.

Per `comp-atomic-swap-procedure`.

Acceptance: patch a tool service while the Maestro is mid-session; verify in-flight calls complete, new calls hit the new version, no session interruption.

Implementation note (2026-05-06): patch reentrancy is now mechanically
serialized and staged binaries must be executable before copy/hash. `jam patch
apply` / `jam patch rollback` acquire `patch-lock/current`, publish lock
events, and release the lock after the operation. Temporary NATS smoke applied
two staged observe versions in sequence and observed two
`patch.lock-acquired`, two `patch.lock-released`, two `patch.applied`, and two
`routing-manifest.updated` events. This task remains in progress until
health-gated service startup, service drain, and the mid-session atomic-swap
acceptance proof land.

Implementation note (2026-05-06): request-reply tool services now support
versioned subscription prefixes through `JAM_<SERVICE>_SUBJECT_PREFIX` with
`JAM_TOOL_SUBJECT_PREFIX` fallback. Observe live-smoke proof used
`tool.observe.v047.>`: versioned health ping returned ok and unversioned
`tool.observe.ping` had no responders, proving side-by-side prefix isolation.

Implementation note (2026-05-06): `jam patch apply` now health-gates candidate
startup. It launches the installed versioned binary with prefix env vars, logs
stdout/stderr under `$JAM_HOME/logs/patch/`, retries `{subject_prefix}.ping`
until timeout, and kills the candidate if health or pre-swap manifest CAS
fails. Live smoke staged the real observe binary, applied version `0.1.0`,
observed manifest revision `routing-manifest:1`, verified
`tool.observe.v010.ping` ok, and verified unversioned `tool.observe.ping` had
no responders. Remaining acceptance work: old-service drain and the
mid-session no-interruption proof.

Implementation note (2026-05-06): old-service drain is now wired. Request-reply
tool services answer `drain` with `status=draining` and exit after the reply;
`jam patch apply` drains the previous route after `patch.applied` and reports a
failure if drain is not acknowledged. Live smoke applied observe `0.0.9` then
`0.1.0`; new `tool.observe.v010.ping` stayed ok, old `tool.observe.v009.ping`
left ok state and then had no responders. Remaining acceptance work: prove the
swap while the Maestro is mid-session and verify no session interruption.

Completion note (2026-05-06): the mid-session proof now passes through
`scripts/smoke-atomic-swap-mid-session.sh`. The smoke starts live NATS, applies
observe `0.0.9`, starts a long-lived Maestro-side router, begins a delayed
in-flight `tool.observe.v009.world-snapshot`, applies observe `0.1.0`, observes
`routing-manifest.updated`, verifies the old in-flight call completed without
an error envelope, verifies the next routed call uses
`tool.observe.v010.world-snapshot`, and verifies old `tool.observe.v009` no
longer answers ok after drain.
