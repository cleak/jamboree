---
id: comp-rollback-flow
type: component
status: planned
created: 2026-05-04T03:39:54.869212908Z
updated: 2026-05-04T04:48:32.263183368Z
edges:
- target: comp-patch-agent
  type: depended_on_by
- target: comp-routing-manifest
  type: depends_on
- target: feat-hot-patching
  type: used_by
---
If post-patch health checks fail (§20.4):

```
1. Read previous_manifest_id from current manifest.
2. Fetch previous manifest from NATS KV history.
3. KV.put with previous manifest contents (atomic).
4. New service notices its subject prefix is no longer in the manifest →
   triggers self-shutdown after drain.
5. Old service was never killed (still subscribed under previous prefix) → resumes.
6. Emit patch.rolled-back with reason.
```

Old service stays alive in the swap window. If health checks fail, point manifest back at it. No state migration needed because subject-prefix-based routing means old and new can coexist.

For services where keeping the old version alive is wasteful (e.g., 2GB-memory observe service): old version is killed at `swap_window_secs` after the patch (default 300s). After that, rollback requires re-launching the old binary from disk — slower but still automatic.