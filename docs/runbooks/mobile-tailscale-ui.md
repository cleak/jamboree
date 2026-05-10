# Mobile UI Over Tailscale

**Status:** Phase 6 operator runbook
**Updated:** 2026-05-06

This runbook covers `task-tailscale-mobile-docs` and the UI auth shape in
`docs/proposal-v5.md` §4.11.1 and §18. The intended exposure is local-first:
bind the UI either to localhost or to the host's Tailscale CGNAT address
(`100.64.0.0/10`), then use a session token from the phone.

Do not bind the UI to `0.0.0.0` for routine use. `jam-ui-server` refuses bind
addresses outside `JAM_UI_ALLOW_BIND_ADDRS` so a bad environment fails at
startup instead of silently exposing the operator surface.

## Preconditions

- Tailscale is installed and connected on the Jamboree host.
- The phone is signed in to the same tailnet.
- Runtime binaries have been installed under `/opt/jam/bin/`.
- The UI static bundle has been deployed to `/home/maestro/.jam/ui/dist`.

## Start On The Tailnet

Find the host's Tailscale IPv4 address:

```bash
tailscale ip -4
```

Start the UI server with that exact address. With process-compose, export the
bind address before starting the substrate:

```bash
export JAM_UI_BIND="$(tailscale ip -4):8787"
export JAM_UI_ALLOW_BIND_ADDRS="127.0.0.1,100.64.0.0/10"
sudo env \
  JAM_UI_BIND="$JAM_UI_BIND" \
  JAM_UI_ALLOW_BIND_ADDRS="$JAM_UI_ALLOW_BIND_ADDRS" \
  /opt/jam/bin/process-compose \
    -U -u /home/maestro/.jam/process-compose.sock \
    up \
    -f /home/caleb/jamboree/process-compose.yaml \
    -D -t=false
```

The allowed bind ranges can also be set in `/home/maestro/.jam/config/ui.toml`:

```toml
[auth]
allow-bind-addrs = ["127.0.0.1", "100.64.0.0/10"]
```

`JAM_UI_ALLOW_BIND_ADDRS` overrides the config file for one-off tests.

For a direct smoke run:

```bash
sudo -u maestro env \
  HOME=/home/maestro \
  JAM_HOME=/home/maestro/.jam \
  JAM_UI_BIND="$(tailscale ip -4):8787" \
  JAM_UI_ALLOW_BIND_ADDRS="127.0.0.1,100.64.0.0/10" \
  NATS_URL=nats://127.0.0.1:4222 \
  /opt/jam/bin/jam-ui-server
```

The server should fail immediately if the bind address is not localhost or a
Tailscale CGNAT address.

## Issue A Phone Token

Issue the token from the same `JAM_HOME` used by the runtime service:

```bash
sudo -u maestro env JAM_HOME=/home/maestro/.jam \
  /opt/jam/bin/jam ui token --user-id human:caleb-phone
```

The token is printed once. Revoke it if the phone is lost or the token leaks:

```bash
sudo -u maestro env JAM_HOME=/home/maestro/.jam \
  /opt/jam/bin/jam ui token-revoke <token-id>
```

For a full reset:

```bash
sudo -u maestro env JAM_HOME=/home/maestro/.jam \
  /opt/jam/bin/jam ui token-revoke-all
```

## Verify From The Phone

Open the browser on the phone:

```text
http://<tailscale-ip>:8787/
```

Paste the token when prompted. The expected checks are:

- `/api/auth/check` accepts the token.
- `/ws` connects and live events appear without polling.
- The notification drawer shows `notify.human` events if the ntfy bridge or
  supervise service publishes one.
- Revoking the token makes a new browser session fail auth.

## Troubleshooting

If the browser cannot connect:

```bash
tailscale status
ss -ltnp | rg ':8787'
curl "http://$(tailscale ip -4):8787/api/health"
```

If auth fails, issue a new token from `/home/maestro/.jam` and verify the
server process is using the same `JAM_HOME`.

If startup fails with `bind address ... is outside allowed UI bind ranges`,
check that `JAM_UI_BIND` is the Tailscale IP, not a LAN IP or `0.0.0.0`. Only
expand `JAM_UI_ALLOW_BIND_ADDRS` for a deliberate, temporary test.
