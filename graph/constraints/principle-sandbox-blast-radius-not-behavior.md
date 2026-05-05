---
id: principle-sandbox-blast-radius-not-behavior
type: constraint
status: active
created: 2026-05-04T03:23:48.099257170Z
updated: 2026-05-04T04:23:33.912064583Z
edges:
- target: comp-docker-sandbox-backend
  type: constrains
- target: comp-local-sandbox-backend
  type: constrains
- target: comp-modal-sandbox-backend
  type: constrains
- target: comp-ssh-sandbox-backend
  type: constrains
- target: feat-picker-layer-three-tier
  type: constrains
- target: feat-sandboxing-profile-x-backend
  type: constrains
---
**§2.4 Sandbox the blast radius, not the behavior.**

Pickers are sandboxed via profile×backend (§6.2). The Maestro is not. Trying to sandbox an agent that needs broad observability creates an arms race; better to make Pickers cheap to contain and Maestro highly trustable.

*Why:* the Maestro reads journals, queries quotas, reads PR comments, talks to Tempyr. Sandboxing all those access paths is high-friction and pressures relaxing sandboxing for capability. Pickers are the actual blast surface — they edit code, run shell commands, push to GitHub. Sandbox there, where containment cost is low.