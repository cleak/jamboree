---
id: task-mechanical-rollback-flow
type: task
status: done
created: 2026-05-04T04:00:27.135822409Z
updated: 2026-05-06T09:00:26Z
edges:
- target: feat-hot-patching
  type: child_of
---
Phase 7 (§12). Mechanical rollback flow.

Per `comp-rollback-flow`.

Old service stays alive in swap window; if health checks fail, point manifest back at it.

Implementation note (2026-05-06): added `jam patch rollback <service> --reason <reason>`. The command reads `routing-manifest/current`, parses `previous_manifest_id`, fetches that exact NATS KV revision from the `routing-manifest` bucket history, compare-and-swap writes it back as current, publishes `routing-manifest.updated`, and emits `patch.rolled-back` with from/to versions and the supplied reason.

Verification (2026-05-06): live smoke with temporary NATS JetStream staged and applied `observe` versions `1.0.0` and `2.0.0`, then ran `jam patch rollback observe --reason "smoke rollback"`. The current manifest returned to `current_version=1.0.0` / `subject_prefix=tool.observe.v100`, and the latest `patch.rolled-back` event recorded `2.0.0 -> 1.0.0`.
