---
id: metric-input-tokens-per-session
type: metric
status: proposed
created: 2026-05-04T03:47:53.660469090Z
updated: 2026-05-04T03:47:53.660469574Z
---
**Per-session input tokens**: 200000 default — warn at this; abort at 2x (§4.1.3, §4.1.4). Configurable via `~/.jam/config/maestro.toml [budget] per-session-input-tokens`.

Sub-caps in `[input-budget]`:
- `skill-files-max-bytes = 80000` (~20K tokens)
- `journal-replay-max-events = 100`
- `world-snapshot-max-bytes = 40000` (~10K tokens)

Loader prioritizes wake context > world-snapshot > scoped skills > journal events. If budget tight, skills truncate first; journal replay second; world-snapshot stays.