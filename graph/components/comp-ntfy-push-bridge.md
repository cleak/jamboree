---
id: comp-ntfy-push-bridge
type: component
status: planned
created: 2026-05-04T03:35:10.904162165Z
updated: 2026-05-04T04:49:06.259027867Z
edges:
- target: comp-jam-ui-server
  type: depended_on_by
- target: feat-budget-enforcement
  type: used_by
- target: feat-failure-handling
  type: used_by
- target: feat-ui-server
  type: used_by
---
ntfy push integration on `notify.human` events (§4.11, §18.5):
- ntfy server URL configurable; default is the public ntfy.sh service with a per-user topic.
- Topic name: `jam-<user-id>-<install-id>` (random component prevents accidental cross-talk).
- Token-protected topic; token in `pass` (`jam/notify/ntfy-token`).
- iOS/Android ntfy app for delivery.
- UI also surfaces the same events in a notification drawer.

Triggered by `notify-human(urgency, summary, payload?)` Maestro tool, plus reconciler escalations (quota exhausted, NTP unsynced, harness drift, patch-failure, tempyr-write-permanently-failed).