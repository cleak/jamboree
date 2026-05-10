---
id: api-nats-bus-subjects-catalog
type: api_surface
status: stable
created: 2026-05-04T03:53:46.190563714Z
updated: 2026-05-10T00:00:00Z
edges:
- target: comp-nats-jetstream
  type: exposed_by
- target: feat-substrate-services
  type: exposed_by
---
Consolidated bus subjects (§21.1):

```
journal.<event-type>                   — durable journal events
picker.<session-id>.lifecycle          — spawn / first-output / exited / killed
picker.<session-id>.output             — stdout/stderr stream
picker.<session-id>.msg.queue|interrupt|kill|status
picker.errored | picker.idle | picker.stalled
quota.<harness>.<event>                — exhausted | refilled | reset | rate-limited | usage-observed
quota.exhausted-soon
tempyr.node-changed | write-pending | write-confirmed | write-permanently-failed
tempyr.update-candidate | tempyr.journal-flushed
evolve.skill-promoted | skill-rejected | skill-under-suspicion
ui.<event>
notify.human
task.failed
patch.staged | applied | confirmed | rolled-back | failed | lock-acquired | lock-released
snapshot.invalidate.<scope>
branch.trunk-moved | branch.staleness-updated
clock.unsynced
harness.version-changed
setup.completed
tool.<service>.<method>
tool.<service>.ping[.<version>]
tool.<service>.drain.<version>
```

Subscription model: durable consumers per service; ephemeral consumers per Maestro session. NATS request-reply for tools; pub/sub for events.

Strict ordering per session-id: `kill` precedence; `queue`/`interrupt` after kill rejected.

Headers: `Trace-Id` (required), `Parent-Trace-Id` (optional), `Schema-Version` (required), `Reply-To` (auto).

Implementation note (2026-05-06): current tool-service subject parsing accepts both `tool.<service>.ping` and `tool.<service>.ping.<version>` for health checks. Routing-manifest calls use `<subject_prefix>.<method>` (for example `tool.observe.v047.world-snapshot`), while the health-check verifier defaults to `tool.<service>.ping` and can target an explicit versioned subject with `--subject`.
