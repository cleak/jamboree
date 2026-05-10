---
id: task-health-check-protocol
type: task
status: done
created: 2026-05-04T04:00:24.926480433Z
updated: 2026-05-06T08:54:07Z
edges:
- target: feat-hot-patching
  type: child_of
---
Phase 7 (§12). Health check protocol per service. Each tool service health-pings on `tool.<service>.ping` every 5s.

Per `feat-tool-services-out-of-process`.

Implementation note (2026-05-06): added `jam health ping <service>` with a 5-second default timeout. It sends a traced NATS request to `tool.<service>.ping` (or an explicit `--subject` such as `tool.observe.ping.v047`), requires `status=ok`, validates the responding service name, and prints subject/service/version/status. Current request-reply tool services (`jam-svc-observe`, `jam-svc-session`, `jam-svc-worktree`, `jam-svc-repo`) all respond to default `ping` plus version-suffixed `ping.<version>` subjects. `process-compose.yaml` now attaches 5-second readiness probes using `jam health ping` to all declared `jam-svc-*` processes; future disabled services will fail loudly until they implement the same protocol.

Verification (2026-05-06): live smoke with temporary NATS JetStream started the four implemented request-reply tool services and verified both default and version-suffixed ping subjects: observe, session, worktree, and repo each returned ok.
