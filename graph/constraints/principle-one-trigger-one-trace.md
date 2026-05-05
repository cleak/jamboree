---
id: principle-one-trigger-one-trace
type: constraint
status: active
created: 2026-05-04T03:23:50.651844685Z
updated: 2026-05-04T04:24:56.039223459Z
edges:
- target: comp-jam-trace-crate
  type: constrains
- target: comp-trace-gap-detector
  type: constrains
- target: comp-trace-replay-tool
  type: constrains
- target: comp-traced-publish-wrapper
  type: constrains
- target: feat-trace-propagation
  type: constrains
---
**One external trigger, one trace.** (§23.1)

Each external trigger — user input, Maestro wake-on-event, periodic tick, reviewer poll, webhook, `jam patch apply`, `jam task spawn`, reconciler run — opens a NEW root trace. Activity within that trigger shares the trace; spawning a child workflow (Picker spawn, patch apply) opens a child trace with `parent_trace_id` pointing at the original.

A specific consequence: when a Picker writes a PR comment and then the `pr-status-poller` later detects that comment, the poller's detection opens a SEPARATE root trace — not a continuation of the original. Cross-trigger correlation happens via `task_id`/`pr_ref`, not `parent_trace_id`. (§24.5 Maestro wake on review walks through this.)

This avoids confusing "trace A caused B" with "B happened later about the thing A touched."