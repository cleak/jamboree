---
id: dec-single-node-jetstream
type: decision
status: decided
created: 2026-05-04T03:46:23.374089495Z
updated: 2026-05-04T05:02:44.767278212Z
edges:
- target: comp-nats-jetstream
  type: decision_for
- target: feat-substrate-services
  type: depended_on_by
---
**Single-node JetStream, no cluster** (§4.4.1, §14).

Why: single-developer single-machine deployment is the target. JetStream durability handles single-node-restart cleanly. Cross-machine clustering is explicitly out of scope (§14).

Loopback-only (TLS not required). Auth: token-based, generated at first install, stored in `pass`.

NATS down for 30 seconds is a hiccup, not an outage (§13.18). Supervisor restart policy gets it back fast.