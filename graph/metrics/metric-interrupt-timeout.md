---
id: metric-interrupt-timeout
type: metric
status: proposed
created: 2026-05-04T03:48:10.540887189Z
updated: 2026-05-04T03:48:10.540888054Z
---
**Interrupt timeout**: 30s default (§5.7 `interrupt-with-message`). If `interrupt-accepted` doesn't arrive within `interrupt_timeout_secs`, surface `interrupt-stuck` event so the Maestro (or human) can escalate to `full-stop`.

Configurable per-harness if needed (some harnesses are slower to acknowledge cancellation).