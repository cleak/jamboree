---
id: comp-routing-manifest
type: component
status: active
created: 2026-05-04T03:31:35.226866599Z
updated: 2026-05-06T08:44:41Z
edges:
- target: comp-atomic-swap-procedure
  type: depended_on_by
- target: comp-jam-svc-evolve
  type: depended_on_by
- target: comp-jam-svc-knowledge
  type: depended_on_by
- target: comp-jam-svc-message
  type: depended_on_by
- target: comp-jam-svc-observe
  type: depended_on_by
- target: comp-jam-svc-repo
  type: depended_on_by
- target: comp-jam-svc-research
  type: depended_on_by
- target: comp-jam-svc-search
  type: depended_on_by
- target: comp-jam-svc-session
  type: depended_on_by
- target: comp-jam-svc-supervise
  type: depended_on_by
- target: comp-jam-svc-worktree
  type: depended_on_by
- target: comp-maestro-session-loop
  type: depended_on_by
- target: comp-patch-agent
  type: depended_on_by
- target: comp-rollback-flow
  type: depended_on_by
- target: dec-tool-services-out-of-process
  type: has_decision
- target: feat-hot-patching
  type: used_by
- target: feat-tool-services-out-of-process
  type: used_by
---
Single source of truth for "which version of which service is current." Stored in NATS KV bucket `routing-manifest` as a single JSON blob (§20.2). Single-writer, atomic update, no distributed transaction needed.

```json
{
  "schema_version": 1,
  "updated_at": "...",
  "updated_by": "human:caleb",
  "trace_id": "01HXKJ...",
  "services": {
    "observe": {
      "current_version": "0.4.7",
      "subject_prefix": "tool.observe.v047",
      "binary_path": "/opt/jam/bin/jam-svc-observe-0.4.7",
      "binary_sha256": "...",
      "started_at": "...",
      "expected_health": "ok"
    },
    "session": { ... }
  },
  "previous_manifest_id": "manif-2026-05-02-15-58-44"
}
```

Maestro reads at session start (cached for session); re-reads on `routing-manifest.updated` events.

Implementation status (2026-05-06): schema-v1 structs and KV read/write helpers live in `crates/jam-nats`. `jam patch apply` performs the first concrete writer path, using NATS KV compare-and-swap for `routing-manifest/current` and publishing `routing-manifest.updated`. The Maestro has a Python `RoutingManifestRouter` with a NATS KV source for routed observe calls; the general multi-tool dispatch loop and rollback history reader remain future slices.
