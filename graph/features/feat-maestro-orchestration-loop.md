---
id: feat-maestro-orchestration-loop
type: feature
status: draft
created: 2026-05-04T03:28:15.472726575Z
updated: 2026-05-04T04:38:12.741501517Z
owner: caleb
edges:
- target: api-maestro-backend-protocol
  type: exposes
- target: comp-litellm-backend
  type: uses
- target: comp-maestro-process
  type: uses
- target: comp-maestro-session-loop
  type: uses
- target: comp-maestro-tempyr-journal-anchor
  type: uses
- target: comp-maestro-wake-handler
  type: uses
- target: dec-chatgpt-subscription-oauth-for-maestro
  type: depends_on
- target: dec-episodic-maestro-sessions
  type: depends_on
- target: dec-litellm-for-maestro
  type: depends_on
- target: jamboree-v5
  type: child_of
- target: principle-agent-first-bounded-supervision
  type: constrained_by
- target: principle-episodic-maestro-sessions
  type: constrained_by
- target: principle-observable-not-deterministic
  type: constrained_by
- target: principle-provider-agnostic-everywhere
  type: constrained_by
- target: principle-rust-trusted-core-python-agent-layer
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
- target: task-litellm-backend-skeleton
  type: parent_of
- target: task-maestro-session-loop
  type: parent_of
- target: the-manager
  type: serves
---
The Maestro is a long-running Python process that runs **episodic** GPT-5.5 sessions via LiteLLM. Each session is opened by a wake event, runs to completion (or interrupt/budget abort), exits cleanly. Between sessions the process is idle. (§4.1)

Wake sources (§4.1.1): bus events on subscribed subjects (`pr.review-received`, `picker.errored`, `picker.idle`, `quota.exhausted-soon`, `tempyr.update-candidate`, `skill.under-suspicion`), direct user input, periodic ticks (5min), stall escalations.

Session lifecycle (§4.1.2): open on wake → load relevant skills + world-snapshot → reason → call tools → close on done/budget/interrupt/fatal.

Persistent state lives in skills, journal, Tempyr, user-edits memory. No persistent in-memory state between sessions — that property is what bounds drift and cost.

Maestro tool surface and system prompt: §5, §8.