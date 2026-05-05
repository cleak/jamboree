---
id: metric-stall-token-idle-secs
type: metric
status: proposed
created: 2026-05-04T03:47:59.457850917Z
updated: 2026-05-04T03:47:59.457851803Z
---
**Stall detector token-idle threshold**: 90s for active turns, 600s for idle waits (§4.4.6).

Other stall criteria:
- Same tool called with same arguments N+ times in a row (default N=3).
- Picker process running but `world-snapshot` hasn't changed in `stall_progress_secs` (default 300s).

On stall: emit `picker.stalled`. Maestro decides response.