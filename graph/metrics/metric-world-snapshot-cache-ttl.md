---
id: metric-world-snapshot-cache-ttl
type: metric
status: proposed
created: 2026-05-04T03:47:57.520931817Z
updated: 2026-05-04T03:47:57.520932436Z
---
**World-snapshot cache TTL backstop**: 60s (§4.2.1, §21.2). Backstop for sources without invalidation events.

Event-driven invalidation (per `dec-event-driven-cache-invalidation`) handles known precise events (PR review, CI status change, Picker spawn, trunk move, Tempyr node change, harness version change, quota change). TTL covers everything else.