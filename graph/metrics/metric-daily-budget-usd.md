---
id: metric-daily-budget-usd
type: metric
status: proposed
created: 2026-05-04T03:47:51.753401859Z
updated: 2026-05-04T03:47:51.753402611Z
---
**Daily USD budget**: $100.00 default (§4.1.4). On 100% trip: emit `maestro.budget.daily-exceeded`, set `dispatch-paused: true` in NATS KV, ntfy human urgently. Maestro refuses to wake until human resumes.

Configurable via `~/.jam/config/maestro.toml [budget] daily-usd`.