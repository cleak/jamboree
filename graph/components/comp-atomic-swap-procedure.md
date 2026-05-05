---
id: comp-atomic-swap-procedure
type: component
status: planned
created: 2026-05-04T03:39:53.582153123Z
updated: 2026-05-04T04:48:24.175764304Z
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