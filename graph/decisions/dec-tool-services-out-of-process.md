---
id: dec-tool-services-out-of-process
type: decision
status: decided
created: 2026-05-04T03:46:04.063014205Z
updated: 2026-05-04T05:00:54.755422491Z
edges:
- target: comp-routing-manifest
  type: decision_for
- target: feat-tool-services-out-of-process
  type: depended_on_by
---
**Move tool services out-of-process** (§v5 changes #1, §4.3, §20). Each tool service is its own Rust process; communication via NATS request-reply.

Why: hot-patching. We need to upgrade the search router or the observation layer without restarting the Maestro or reconciling with running Pickers. In-process linkage forces system-wide restarts; out-of-process atomic-swap doesn't.

v4 had `jam-tools-*` as in-process Rust crates linked into one binary. v5 makes each its own process.