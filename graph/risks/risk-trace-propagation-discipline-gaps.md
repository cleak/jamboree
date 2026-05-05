---
id: risk-trace-propagation-discipline-gaps
type: risk
status: identified
created: 2026-05-04T03:47:14.405702579Z
updated: 2026-05-04T03:47:14.405702899Z
---
**§13.15 Trace propagation discipline gaps (NEW v5).** A service that emits an event without `trace_id` breaks the chain.

Mitigation: NATS publish wrapper rejects publishes without `trace_id`; event-emit helpers in every service require `trace_id` parameter (no default); `tempyr journal lint` corollary catches single-entry traces; `jam doctor` includes trace propagation health checks; integration tests verify end-to-end trace continuity for a sample task.

Captured as `principle-tracing-chains-end-to-end`.