---
id: principle-failure-surfaces-immediately
type: constraint
status: active
created: 2026-05-04T03:23:49.118761726Z
updated: 2026-05-04T04:32:26.482524592Z
edges:
- target: comp-bootstrap-users-sh
  type: constrains
- target: comp-harness-version-lockfile
  type: constrains
- target: comp-jam-svc-evolve
  type: constrains
- target: comp-jam-svc-knowledge
  type: constrains
- target: comp-jam-svc-message
  type: constrains
- target: comp-jam-svc-observe
  type: constrains
- target: comp-jam-svc-repo
  type: constrains
- target: comp-jam-svc-research
  type: constrains
- target: comp-jam-svc-search
  type: constrains
- target: comp-jam-svc-session
  type: constrains
- target: comp-jam-svc-supervise
  type: constrains
- target: comp-jam-svc-worktree
  type: constrains
- target: comp-jam-ui-server
  type: constrains
- target: comp-patch-agent
  type: constrains
- target: comp-search-router
  type: constrains
- target: comp-ui-session-token-auth
  type: constrains
- target: comp-worktree-create-protocol
  type: constrains
- target: feat-budget-enforcement
  type: constrains
- target: feat-event-schema-versioning
  type: constrains
- target: feat-failure-handling
  type: constrains
- target: feat-hot-patching
  type: constrains
- target: feat-input-budget-management
  type: constrains
- target: feat-jam-cli
  type: constrains
- target: feat-live-update-flows
  type: constrains
- target: feat-messaging-three-modes
  type: constrains
- target: feat-multi-user-security-model
  type: constrains
- target: feat-observation-tool-service
  type: constrains
- target: feat-picker-layer-three-tier
  type: constrains
- target: feat-reviewer-adapters
  type: constrains
- target: feat-search-router
  type: constrains
- target: feat-substrate-services
  type: constrains
- target: feat-tech-stack-hardening
  type: constrains
- target: feat-tempyr-consistency-model
  type: constrains
- target: feat-tool-services-out-of-process
  type: constrains
- target: feat-trace-propagation
  type: constrains
- target: feat-ui-server
  type: constrains
---
**§2.12 Failure surfaces immediately or not at all.**

Every component fails loudly with a specific reason and (where possible) a remediation hint. Silent degradation is worse than crashing — a crash gets noticed and fixed; silent degradation produces bad outputs that look fine.

When a component cannot operate correctly, it refuses to operate, journals the refusal with diagnostic detail, and surfaces a notification. This applies to setup checks, tool services, reconcilers, the patch agent, and trace-gap detection.

*Why:* the failure modes that actually hurt are the ones nobody notices until much later. Loud failures get fixed in minutes; silent failures rot for weeks. The worst possible outcome is "the orchestrator was running fine all weekend but every PR it produced was subtly broken." This principle prevents that class of outcome at the cost of slightly more aggressive crashes during development.

Implementer's checklist: §10.4.