---
id: principle-observable-not-deterministic
type: constraint
status: active
created: 2026-05-04T03:23:47.845877737Z
updated: 2026-05-04T04:17:30.032248706Z
edges:
- target: feat-live-update-flows
  type: constrains
- target: feat-maestro-orchestration-loop
  type: constrains
- target: feat-observation-tool-service
  type: constrains
- target: feat-quota-tracking
  type: constrains
---
**§2.1 More observable, not more deterministic.**

The Maestro is an agent, not a state machine. The fix for fragmented context is not adding deterministic workflow steps; the fix is giving the Maestro a *better* view of reality. Every Maestro decision starts with `world-snapshot(task-id-or-pr-url)` — a single typed call that compiles current truth from git, GitHub, CI, CodeRabbit, journals, quota, branch-staleness, and Tempyr into one coherent object.

This is a fact compiler, not a state machine. The Maestro can disagree, override blockers, escalate — but always against a concrete reference point.

*Why load-bearing:* the alternative (Maestro pokes at git/GitHub/journals individually) produces incoherent context. By the time it has the picture, it's stale. A snapshot is one-shot; no half-loaded state.