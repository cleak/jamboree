---
id: jamboree-v5
type: epic
status: active
created: 2026-05-04T03:23:47.776594250Z
updated: 2026-05-04T05:06:57.890348305Z
owner: caleb
edges:
- target: feat-budget-enforcement
  type: parent_of
- target: feat-deep-research
  type: parent_of
- target: feat-event-schema-versioning
  type: parent_of
- target: feat-failure-handling
  type: parent_of
- target: feat-hot-patching
  type: parent_of
- target: feat-implementation-walkthrough-reference
  type: parent_of
- target: feat-input-budget-management
  type: parent_of
- target: feat-jam-cli
  type: parent_of
- target: feat-jam-prefix-naming-rule
  type: parent_of
- target: feat-live-update-flows
  type: parent_of
- target: feat-maestro-orchestration-loop
  type: parent_of
- target: feat-maestro-tool-surface
  type: parent_of
- target: feat-mcp-integration
  type: parent_of
- target: feat-messaging-three-modes
  type: parent_of
- target: feat-monorepo-layout
  type: parent_of
- target: feat-multi-user-security-model
  type: parent_of
- target: feat-observation-tool-service
  type: parent_of
- target: feat-picker-layer-three-tier
  type: parent_of
- target: feat-quota-tracking
  type: parent_of
- target: feat-record-learning
  type: parent_of
- target: feat-reviewer-adapters
  type: parent_of
- target: feat-sandboxing-profile-x-backend
  type: parent_of
- target: feat-search-router
  type: parent_of
- target: feat-self-improvement
  type: parent_of
- target: feat-skill-evolution-pipeline
  type: parent_of
- target: feat-substrate-services
  type: parent_of
- target: feat-task-tracking-via-lifecycle-transitions
  type: parent_of
- target: feat-tech-stack-hardening
  type: parent_of
- target: feat-tempyr-consistency-model
  type: parent_of
- target: feat-tempyr-knowledge-and-journal
  type: parent_of
- target: feat-tool-services-out-of-process
  type: parent_of
- target: feat-trace-propagation
  type: parent_of
- target: feat-ui-server
  type: parent_of
- target: note-architecture-overview
  type: relates_to
- target: note-bottom-line
  type: relates_to
- target: note-build-order-phases
  type: relates_to
---
Implementation of the Jamboree multi-coding-agent orchestrator per `docs/proposal-v5.md` (§0–§24, ~4400 lines, dated 2026-05-03), the security-setup addendum, and the monorepo layout decision.

The Maestro (Python orchestrator) drives many sandboxed Pickers (Codex CLI, Claude Code, OpenCode+DeepSeek) in parallel against Caleb's Bevy/Rust voxel game *Blueberry*, with quota-aware dispatch, agent-in-the-loop supervision over a typed observation surface, durable knowledge in Tempyr, and full chain traceability.

Key v5 shifts vs v4:
- Tool services moved out-of-process (atomic-swappable via routing manifest).
- Tempyr canonical worktree pattern (three-checkout geography).
- Patch agent + atomic-upgrade infrastructure with deterministic-then-LLM recovery.
- Tempyr journal as the agent reasoning layer.
- Trace-id propagation as load-bearing (§2.13).
- Failure-obvious as load-bearing (§2.12).

Initial target: Blueberry. Substrate runs as Linux user `maestro` (UID 2000); Pickers as `picker` (UID 2001). Source-of-truth lives in `/home/caleb/jamboree/`; runtime deploys to `/home/maestro/.jam/`.