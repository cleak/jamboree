---
id: the-manager
type: persona
created: 2026-05-04T03:23:47.715518971Z
updated: 2026-05-04T04:06:17.363038986Z
edges:
- target: feat-budget-enforcement
  type: served_by
- target: feat-failure-handling
  type: served_by
- target: feat-implementation-walkthrough-reference
  type: served_by
- target: feat-input-budget-management
  type: served_by
- target: feat-jam-cli
  type: served_by
- target: feat-maestro-orchestration-loop
  type: served_by
- target: feat-messaging-three-modes
  type: served_by
- target: feat-multi-user-security-model
  type: served_by
- target: feat-self-improvement
  type: served_by
- target: feat-trace-propagation
  type: served_by
- target: feat-ui-server
  type: served_by
---
The single human in the loop. Books gigs (queues tasks), funds the budget, gets paged when things go wrong, signs off on encores (PR merges).

Linux user `caleb` (UID 1000) drives the `jam` CLI as the Manager. Per spec §0.0 naming and CLAUDE.md naming table. Distinct from the Maestro (orchestrator agent) and the Pickers (sandboxed coding agents) — those are agents, not personas.

The Manager's responsibilities:
- Issue work via `jam task spawn` and the UI.
- Review and merge PRs (the only hard human gate; no `merge-pr` tool exists per §2.3).
- Review skill-evolution candidates and Tempyr update candidates.
- Respond to ntfy escalations (urgency=critical patch failures, all-quota-exhausted, NTP unsynced).
- Edit skills (`~caleb/code/jam-skills/`) and Tempyr nodes (`~caleb/code/blueberry-tempyr-live/tempyr/nodes`).

Single-developer system, single-machine deployment (security-setup §1).