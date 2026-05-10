---
id: task-tailscale-mobile-docs
type: task
status: blocked
created: 2026-05-04T04:00:09.912025759Z
updated: 2026-05-06T15:55:57Z
edges:
- target: feat-ui-server
  type: child_of
---
Phase 6 (§12). Tailscale documentation for mobile setup.

Acceptance: open UI on phone via Tailscale; verify session token works from CGNAT range (`100.64.0.0/10`).

Implementation note (2026-05-06): added
`docs/runbooks/mobile-tailscale-ui.md` and startup bind-address enforcement in
`jam-ui-server`. Allowed ranges load from `$JAM_HOME/config/ui.toml` or
`JAM_UI_ALLOW_BIND_ADDRS`; the default is `127.0.0.1,100.64.0.0/10`.
Unsafe binds such as `0.0.0.0:8787` fail loudly.
Smoke verified the guard locally.

Blocked note (2026-05-06): host verification cannot be completed here because
`tailscale` is not installed (`tailscale ip -4` returns command-not-found) and
`ip -4 addr show` has no `100.64.0.0/10` interface. To finish acceptance,
install/connect Tailscale on this host, start `jam-ui-server` bound to the
reported Tailscale IPv4 address, issue a `jam ui token` from the runtime
`JAM_HOME`, and verify `/api/auth/check` plus `/ws` from the phone browser.
