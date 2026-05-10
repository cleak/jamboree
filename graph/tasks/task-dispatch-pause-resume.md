---
id: task-dispatch-pause-resume
type: task
status: done
created: 2026-05-04T04:01:27.211306420Z
updated: 2026-05-06T10:54:09.464810283Z
---
Implement `pause-dispatch(reason)` / `resume-dispatch()` that toggles `dispatch-paused: bool` in NATS KV bucket `dispatch-state`.

Per `api-pause-dispatch`. Triggered automatically on daily-budget-exceeded and on patch-agent failure escalation.

Implementation note (2026-05-06): `jam pause-dispatch --reason ...` and `jam resume-dispatch` now write the durable `dispatch-paused` bool in the NATS KV bucket `dispatch-state`, plus a structured `state` record with `reason`, `changed_at`, and `changed_by`. `jam task spawn` reads this KV state before publishing `journal.task.requested` and refuses while dispatch is paused.

Live smoke (2026-05-06): temporary NATS root `/tmp/jam-dispatch-pause-smoke-jOFaD5` on port `43227` verified the CLI toggle. `jam pause-dispatch --reason 'dispatch pause smoke'` wrote `dispatch_paused: true`; `jam task spawn 'should be blocked while dispatch paused'` exited `1` with `dispatch paused since 2026-05-06T07:18:25Z by human:caleb: dispatch pause smoke`; `jam resume-dispatch` wrote `dispatch_paused: false`; the next `jam task spawn 'allowed after dispatch resume'` succeeded with trace `01KQY2DXHFJF913SD01JKQ07PR`, and `jam-nats-bridge` wrote exactly one `journal.task.jsonl` entry.

Implementation note (2026-05-06): `jam-svc-supervise` now exposes traced `tool.supervise.pause-dispatch` and `tool.supervise.resume-dispatch` request/reply methods. They write the same `dispatch-state` KV keys as the CLI path (`dispatch-paused` and structured `state`) and return the `dispatch_paused`, `reason`, `changed_at`, and `changed_by` record. The Maestro typed tool registry includes `pause-dispatch` / `resume-dispatch`.

Live smoke (2026-05-06): temporary NATS root `/tmp/jam-supervise-dispatch-smoke-itClrK` on port `58095` verified the service toggle. A traced `tool.supervise.pause-dispatch` request returned `dispatch_paused=true`; `jam task spawn 'blocked by supervise service pause'` exited `1` with `dispatch paused since 2026-05-06T10:52:50Z by human:caleb: supervise dispatch smoke`; a traced `tool.supervise.resume-dispatch` request returned `dispatch_paused=false`; the next `jam task spawn 'allowed after supervise service resume'` succeeded with trace `01KQYEQ3PX0FT4NX8J1KG8SFPJ`.
