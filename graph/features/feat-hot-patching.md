---
id: feat-hot-patching
type: feature
status: draft
created: 2026-05-04T03:28:23.301350177Z
updated: 2026-05-04T05:05:42.162164116Z
owner: caleb
edges:
- target: comp-atomic-swap-procedure
  type: uses
- target: comp-patch-agent
  type: uses
- target: comp-rollback-flow
  type: uses
- target: comp-routing-manifest
  type: uses
- target: dec-patch-agent-deterministic-then-llm
  type: depends_on
- target: insight-deterministic-then-llm-pattern
  type: informed_by
- target: jamboree-v5
  type: child_of
- target: principle-decoupled-processes-bus
  type: constrained_by
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
- target: task-atomic-swap-procedure-impl
  type: parent_of
- target: task-health-check-protocol
  type: parent_of
- target: task-jam-patch-agent-impl
  type: parent_of
- target: task-llm-diagnosis-flow
  type: parent_of
- target: task-mechanical-rollback-flow
  type: parent_of
- target: task-patch-event-vocabulary
  type: parent_of
- target: task-routing-manifest-schema
  type: parent_of
---
Atomic upgrade of tool services without restarting the Maestro or impacting in-flight Pickers (§20).

**Routing manifest** in NATS KV (`routing-manifest` bucket): single JSON blob with `{services: {<svc>: {current_version, subject_prefix, binary_path, binary_sha256, started_at, expected_health}, ...}, previous_manifest_id}`. Single-writer atomic update via compare-and-swap.

**Atomic-swap procedure** (§20.3): verify staged binary → generate new subject prefix `tool.<svc>.v<new>` → start new service → wait ≤30s for first health ping → KV.put with revision check → emit `patch.applied` → old service drains.

**Rollback** (§20.4): read `previous_manifest_id`, fetch from KV history, KV.put atomic. Old service is still alive in swap window; new one self-shutdowns after drain.

**Patch agent** (§20.5, separate Rust crate, pinned deps `tokio`, `serde`, `tracing`, `nats`, `octocrab`, one LLM client): deterministic checks (cheap, ~80% of failures) → mechanical rollback → LLM diagnosis ($0.50 budget cap, single-turn) → ntfy escalation with incident dump (`~/.jam/incidents/<id>/`).

Reentrancy: one patch in flight at a time, NATS KV `patch-lock` bucket TTL 5min.