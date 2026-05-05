---
id: feat-tool-services-out-of-process
type: feature
status: draft
created: 2026-05-04T03:28:16.598357204Z
updated: 2026-05-04T04:36:33.821032719Z
owner: caleb
edges:
- target: comp-events-toml-and-codegen
  type: uses
- target: comp-jam-svc-evolve
  type: uses
- target: comp-jam-svc-knowledge
  type: uses
- target: comp-jam-svc-message
  type: uses
- target: comp-jam-svc-observe
  type: uses
- target: comp-jam-svc-repo
  type: uses
- target: comp-jam-svc-research
  type: uses
- target: comp-jam-svc-search
  type: uses
- target: comp-jam-svc-session
  type: uses
- target: comp-jam-svc-supervise
  type: uses
- target: comp-jam-svc-worktree
  type: uses
- target: comp-nats-tool-rpc
  type: uses
- target: comp-routing-manifest
  type: uses
- target: dec-tool-services-out-of-process
  type: depends_on
- target: jamboree-v5
  type: child_of
- target: principle-decoupled-processes-bus
  type: constrained_by
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-native-fs-only
  type: constrained_by
- target: principle-rust-trusted-core-python-agent-layer
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
---
Each tool service is its own Rust process; communication via NATS request-reply (§4.3, §2.5). On startup each service: validates paths (§2.14), connects NATS, subscribes `tool.<service>.*`, loads its routing manifest entry, health-pings every 5s on `tool.<service>.ping`, refuses to start on any check failure (§2.12).

Services: `jam-svc-observe`, `-session`, `-worktree`, `-repo`, `-knowledge`, `-search`, `-research`, `-message`, `-supervise`, `-evolve` (full table in §4.3).

Tools exposed by each service are JSON-schema-described in `crates/jam-tools-core/schemas/<service>/<tool>.json`. Schemas drive both Rust types (`schemars`) and Pydantic types (build script). Single source of truth.

Out-of-process is the prerequisite for hot-patching (§20) — atomic-swap of one service binary while the Maestro session continues.