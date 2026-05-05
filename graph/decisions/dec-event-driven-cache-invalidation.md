---
id: dec-event-driven-cache-invalidation
type: decision
status: decided
created: 2026-05-04T03:46:37.081730260Z
updated: 2026-05-04T05:03:40.303868947Z
edges:
- target: comp-world-snapshot-cache
  type: decision_for
---
**World-snapshot cache: event-driven invalidation backed by 60s TTL** (§4.2.1, §21.2). v4 used pure 60s TTL; v5 makes the cache subscribe to events that imply staleness.

Why: TTL alone creates the staleness window the human worries about. Picker spawn, PR comment, CI status change — these are precisely-known moments. Subscribing the cache to those events means the Maestro never reads a snapshot that's outdated relative to a known event.

TTL stays as backstop for sources we don't have events for. The `freshness` field per data source means the Maestro always knows what's fresh.