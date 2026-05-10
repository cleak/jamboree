---
id: task-notify-human-ntfy
type: task
status: blocked
created: 2026-05-04T03:59:19.098639706Z
updated: 2026-05-06T19:20:10Z
edges:
- target: feat-failure-handling
  type: child_of
---
Phase 3 (§12). `notify-human` via ntfy bridge.

Per `comp-ntfy-push-bridge`, `api-notify-human`.

Acceptance: Maestro calls `notify-human(urgency=high, summary="...")`; ntfy push delivered to phone; UI surfaces same event in notification drawer.

Implementation note (2026-05-06): added `jam-svc-supervise` for traced `tool.supervise.notify-human` requests and `jam-ntfy-bridge` for `notify.human` → ntfy POST delivery. Added Maestro `notify-human` to the typed tool registry and added the SolidJS notification drawer fed by the authenticated WebSocket bridge. Live smoke used temporary NATS plus fake `curl`: the tool call returned `status=published`, `jam-ntfy-bridge` consumed `notify.human`, and fake `curl` recorded ntfy `Authorization`, `Title`, `Priority`, `Tags`, body, and topic URL.

Credential note (2026-05-06): local runtime now has `jam/notify/ntfy-token` in
maestro pass, so the bridge can resolve its token through the normal pass
backend.

Blocked note (2026-05-06): real phone delivery still needs an explicit mobile
subscription/topic verification and the production substrate running. To finish
acceptance, enable `jam-ntfy-bridge`, publish a high-urgency `notify-human`
request, and verify it reaches the phone and the UI drawer.
