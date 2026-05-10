---
id: comp-ui-session-token-auth
type: component
status: active
created: 2026-05-04T03:35:09.860153325Z
updated: 2026-05-06T14:18:59Z
edges:
- target: comp-jam-ui-server
  type: depended_on_by
- target: feat-ui-server
  type: used_by
- target: principle-failure-surfaces-immediately
  type: constrained_by
---
Session tokens issued by `jam ui token` (§4.11.1):
- `jam ui token` — generates token, prints once, copies to clipboard.
- `jam ui token --revoke <id>`
- `jam ui token --revoke-all`

User pastes token into the UI on first connect (saved to localStorage thereafter); subsequent reconnects use the saved token. WebSocket handshake verifies token.

Per-user attribution: each token has an associated `user-id`. Actions taken via that token are journaled with `from: human, user-id: <id>`.

`allow-bind-addrs = ["127.0.0.1", "100.64.0.0/10"]` (localhost + Tailscale CGNAT range) is defense-in-depth: even if a token leaks, it's only usable from within trusted network ranges.

Why session tokens now even though it's single-user: the cost is small; the future-proofing is real. A leaked token + network access = full UI access including `full-stop` on Pickers.

Implementation note (2026-05-06): file-backed token issuance/revocation and
server-side verification are implemented in `crates/jam-ui-server`; CLI
issuance/revocation is exposed through `jam ui token*`. `jam-ui-server` now
enforces `allow-bind-addrs` at startup from `$JAM_HOME/config/ui.toml` or
`JAM_UI_ALLOW_BIND_ADDRS`, defaulting to localhost plus Tailscale CGNAT
(`127.0.0.1,100.64.0.0/10`).
