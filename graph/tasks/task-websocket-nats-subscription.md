---
id: task-websocket-nats-subscription
type: task
status: done
created: 2026-05-04T03:59:55.542656980Z
updated: 2026-05-06T08:16:23Z
edges:
- target: feat-ui-server
  type: child_of
---
Phase 6 (§12). WebSocket subscription to NATS subjects.

Per `comp-ui-websocket-nats-bridge`.

Implementation note (2026-05-06): `crates/jam-ui-server` exposes `GET /ws?token=...&subject=...`; the handler verifies the session token, subscribes to the requested NATS subject with the shared `JamNats` client, and forwards each message as JSON `{subject, payload}` over the WebSocket. The SolidJS shell stores the token in localStorage and connects to the bridge with the selected subject.

Live smoke: `/tmp/jam-ui-ws-smoke-4n11dl` ran `/tmp/jam-substrate/bin/nats-server` on port 52345 and `target/debug/jam-ui-server` on port 49057 with a temporary `JAM_HOME`. A token issued by `target/debug/jam ui token` authenticated a raw WebSocket client; publishing core NATS payload `{"smoke":"ws-nats"}` to `journal.test` produced WebSocket frame `{"subject":"journal.test","payload":"{\"smoke\":\"ws-nats\"}"}`.

Verification: `cargo test -p jam-ui-server`, `cargo clippy -p jam-ui-server --all-targets -- -D warnings`, plus the live smoke above.
