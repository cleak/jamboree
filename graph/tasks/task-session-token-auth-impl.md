---
id: task-session-token-auth-impl
type: task
status: done
created: 2026-05-04T04:00:03.888363769Z
updated: 2026-05-06T08:08:24Z
edges:
- target: feat-ui-server
  type: child_of
---
Phase 6 (§12). Session token auth.

Per `comp-ui-session-token-auth`.

Acceptance: `jam ui token` issues / revokes tokens. WebSocket handshake verifies token. Token revocation works.

Implementation note (2026-05-06): token auth is active in `jam-ui-server` and `jam-cli`. `jam ui token`, `jam ui token-revoke`, and `jam ui token-revoke-all` use `TokenStore` under `$JAM_HOME/ui/session-tokens.json`; only SHA-256 hashes persist. `/api/auth/check` and `/ws` share the same `token_is_valid` verifier, so revoked tokens are rejected by the server path before WebSocket upgrade. Coverage: `crates/jam-ui-server/src/auth.rs` tests issue/verify/revoke behavior, and `crates/jam-ui-server/src/main.rs` tests issued tokens pass and revoked tokens fail through the server verifier.

Verification: `cargo fmt --all -- --check`, `cargo clippy -p jam-ui-server --all-targets -- -D warnings`, and `cargo test -p jam-ui-server`.
