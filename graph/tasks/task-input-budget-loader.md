---
id: task-input-budget-loader
type: task
status: done
created: 2026-05-04T04:01:21.734483706Z
updated: 2026-05-06T07:48:51.144563573Z
---
Implement session-start input budget loader: assembles input within budget, prioritizing wake-event context > world-snapshot > scoped skills > journal events.

Per `feat-input-budget-management`, `metric-input-tokens-per-session`.

If budget tight: skills truncate first; journal replay second; world-snapshot stays.

Implementation note (2026-05-06): `maestro/src/jam_maestro/input_budget.py` now loads `[budget]` and `[input-budget]` from `maestro.toml` (`JAM_MAESTRO_CONFIG` or `$JAM_HOME/config/maestro.toml`) with §4.1.3 defaults. `assemble_session_input` creates a budgeted bundle in priority order: wake context, world snapshot, scoped skills, journal events. It enforces `world-snapshot-max-bytes`, `skill-files-max-bytes`, `journal-replay-max-events`, and the per-session input token cap using the local 4 bytes/token estimate. Wake context and world snapshot are retained even under extreme budget pressure; skills and journal events are reduced and reported.

Verification (2026-05-06): added `maestro/tests/unit/test_input_budget.py` for TOML config loading, skill truncation, journal replay limiting, and world-snapshot retention under a tiny total budget. `MaestroSessionLoop` now includes an `input_budget` report in `SessionDecision`, and `test_session_loop_reports_budgeted_skill_input` verifies the session loop uses the budgeted skill bundle.
