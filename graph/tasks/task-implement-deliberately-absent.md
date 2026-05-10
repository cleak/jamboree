---
id: task-implement-deliberately-absent
type: task
status: done
created: 2026-05-04T04:01:46.383034088Z
updated: 2026-05-06T08:06:47Z
---
Audit Maestro tool surface for **deliberately absent** tools (§5.9). These must NOT exist:
- `read-file`, `write-file`, `run-command` (Pickers do file ops, not Maestro)
- `merge-pr` (only `request-human-merge`)
- `add-tool` at runtime (use `propose-tool-change`)
- `eval`, `exec`, `python -c` (banned at lint level)
- `set-task-plan-note` (task plans are session-scoped)
- `auto-rebase`, `auto-merge`, `auto-update-tempyr-node`
- `fork-Maestro`, `clone-session`

Per `principle-structure-in-tools-not-policy`, `insight-no-tool-no-possibility`.

Acceptance: code review confirms none of these exist in the tool registry; tests verify by name that calling them produces "no such tool" errors.

Implementation note (2026-05-06): added the first Maestro-side callable tool allowlist in `maestro/src/jam_maestro/tool_registry.py`. `MaestroToolRegistry` maps the currently callable tool names to their NATS subjects and generated Pydantic request models, rejects any registered name that intersects `DELIBERATELY_ABSENT_TOOL_NAMES`, and raises `NoSuchToolError("no such tool: <name>")` for unregistered calls. `maestro/tests/unit/test_tool_registry.py` parametrically verifies every deliberately absent name (`merge-pr`, `read-file`, `run-command`, `eval`, `exec`, `auto-rebase`, etc.) is not registered and returns the stable no-such-tool error when called.

Verification: `uv run pytest`, `uv run pyright`, and `uv run ruff check`.
