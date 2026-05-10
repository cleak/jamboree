---
id: comp-atomic-swap-procedure
type: component
status: active
created: 2026-05-04T03:39:53.582153123Z
updated: 2026-05-06T15:11:47Z
edges:
- target: comp-patch-agent
  type: depended_on_by
- target: comp-routing-manifest
  type: depends_on
- target: feat-hot-patching
  type: used_by
---
Triggered by `jam patch apply <service> <version>`, reconciler detecting binary update in `~/.jam/staging/`, or future CI. Steps (§20.3):

1. Verify staged binary: exists, executable, SHA256 matches, `<staging-path> --self-test` exits 0.
2. Generate new subject prefix: `tool.<service>.v<new-version>` (guarantees no collision).
3. Start new service with new prefix; subscribes `<prefix>.<method>`; reports health on `tool.<service>.ping.<new-version>`.
4. Wait up to 30s for first health ping. If absent: kill new service, leave manifest unchanged, emit `patch.failed`.
5. Atomic manifest swap via NATS KV compare-and-swap. Retry from step 4 if CAS fails (someone else patched concurrently).
6. Emit `patch.applied` event with trace_id.
7. Old service drains: subscribes to `tool.<service>.drain.<old-version>` signal, stops accepting new requests (returns 503-equivalent), finishes in-flight, exits cleanly.
8. Patch agent observes `patch.applied`, runs health checks (§20.5).

Reentrancy: only one patch in flight at a time. Patch agent acquires NATS KV `patch-lock` (TTL 5min) before applying. Concurrent attempts are queued.

Implementation note (2026-05-06): `jam patch apply` now rejects staged
binaries that are missing or not executable before copying them into the
runtime bin dir. `jam patch apply` and `jam patch rollback` acquire
`patch-lock/current` before mutating staged binaries or routing manifests,
publish traced `patch.lock-acquired` / `patch.lock-released`, and release the
lock after success or failure. A temporary NATS smoke applied `observe`
versions `0.4.7` then `0.4.8`; both applies emitted lock acquired/released,
`patch.applied`, and `routing-manifest.updated`, proving the first apply
released the lock before the second acquired it.

Request-reply tool services now accept versioned subscription prefixes via
`JAM_<SERVICE>_SUBJECT_PREFIX` and shared `JAM_TOOL_SUBJECT_PREFIX` fallback:
observe, worktree, session, repo, search, supervise, and message can subscribe
to `tool.<service>.v<version>.>` while preserving method parsing for
`ping`/`drain`. Temporary NATS smoke started `jam-svc-observe` with
`JAM_OBSERVE_SUBJECT_PREFIX=tool.observe.v047`; `jam health ping observe
--subject tool.observe.v047.ping` returned ok, while unversioned
`tool.observe.ping` had no responders.

`jam patch apply` now starts the installed candidate binary before the
manifest swap, injects `NATS_URL`, `JAM_TOOL_SUBJECT_PREFIX`, and
`JAM_<SERVICE>_SUBJECT_PREFIX`, writes candidate logs under
`$JAM_HOME/logs/patch/`, retries `{subject_prefix}.ping` until the health
timeout, and kills the candidate if health or manifest CAS fails before the
swap. Temporary NATS smoke staged the real `jam-svc-observe` binary at version
`0.1.0`; apply launched `tool.observe.v010`, wrote `routing-manifest:1`, and
post-apply versioned health ping returned ok while unversioned `tool.observe`
had no responders.

Request-reply tool services now handle `drain` by replying with
`{"status":"draining"}` and then exiting after the reply path. `jam patch
apply` rejects launching a duplicate current subject prefix, sends a traced
drain request to the previous manifest route after `patch.applied`, and fails
loudly if the previous service does not acknowledge drain. Temporary NATS smoke
applied observe `0.0.9` then `0.1.0`; `tool.observe.v010.ping` stayed ok while
`tool.observe.v009.ping` moved out of ok and then returned no responders.

Mid-session proof is repeatable through
`scripts/smoke-atomic-swap-mid-session.sh`: a long-lived Maestro-side routing
client opens an in-flight `observe.world-snapshot` call on
`tool.observe.v009`, subscribes for `routing-manifest.updated`, `jam patch
apply observe 0.1.0` swaps to `tool.observe.v010`, the old in-flight call
completes without an error envelope, the router reloads from the update event,
the next call uses `tool.observe.v010`, and old `tool.observe.v009` no longer
answers ok after drain.
