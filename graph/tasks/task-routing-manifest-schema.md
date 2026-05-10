---
id: task-routing-manifest-schema
type: task
status: done
created: 2026-05-04T04:00:12.870352660Z
updated: 2026-05-06T08:44:41Z
edges:
- target: feat-hot-patching
  type: child_of
---
Phase 7 (§12). Routing manifest schema in NATS KV.

Per `comp-routing-manifest`, `dec-tool-services-out-of-process`.

Acceptance: `jam patch apply <service> <version>` writes new manifest; Maestro re-reads on `routing-manifest.updated` events; next tool call uses new prefix.

Implementation note (2026-05-06): `jam-nats` owns the schema-v1 Rust manifest model and KV helpers for `routing-manifest/current`, with CAS writes and conventional versioned prefixes such as `tool.observe.v047`. `jam patch apply <service> <version>` now copies the staged binary from `$JAM_HOME/staging/jam-svc-<service>-<version>` into `$JAM_HOME/bin/`, records its SHA-256 in the manifest, publishes `routing-manifest.updated`, and emits `patch.applied`. The Python Maestro loads the manifest through JetStream API reads and routes `observe.world-snapshot` through `<subject_prefix>.world-snapshot`, falling back to `tool.observe.world-snapshot` only when no manifest route exists.

Verification (2026-05-06): live smoke with temporary NATS JetStream wrote `routing-manifest/current` for `observe` version `9.9.9`, observed `routing-manifest.updated`, and confirmed `python -m jam_maestro world-snapshot` called `tool.observe.v999.world-snapshot` and received the routed response.
