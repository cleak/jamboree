---
id: risk-nats-server-spof
type: risk
status: accepted
created: 2026-05-04T03:47:19.769595615Z
updated: 2026-05-04T03:47:19.769596564Z
---
**§13.18 NATS server as single point of failure (NEW v5).** NATS down = nothing works.

Mitigation: NATS is exceptionally stable; JetStream durability means restart resumes cleanly; supervisor restart policy gets it back fast; severity-aligned: NATS down for 30 seconds is a hiccup, not an outage.