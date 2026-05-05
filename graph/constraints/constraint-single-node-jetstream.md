---
id: constraint-single-node-jetstream
type: constraint
status: active
created: 2026-05-04T03:23:51.602494457Z
updated: 2026-05-04T04:27:55.349517368Z
edges:
- target: comp-nats-jetstream
  type: constrains
- target: feat-substrate-services
  type: constrains
---
NATS deployment is single-node JetStream, local machine. No cluster (§4.4.1, §14). TLS not required for loopback; auth is token-based with a strong token generated at install time, stored in `pass`.

Cross-machine NATS clustering is explicitly deferred (§14).

NATS-down is mitigated by JetStream durability — subscribers resume from last-acknowledged offset on restart; supervisor restart policy gets it back fast (§13.18).