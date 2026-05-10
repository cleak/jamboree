---
id: task-record-learning-tool
type: task
status: done
created: 2026-05-04T04:01:49.611225301Z
updated: 2026-05-06T08:13:15Z
---
Implement `record-learning(scope, evidence, guidance, counterexample, confidence, originated-from-trace?)`. Writes a structured skill note as markdown AND emits a Tempyr `decision`/`finding` entry tagged with the skill scope and the new skill's path.

Per `feat-record-learning`, `dec-record-learning-emits-dual`, `api-record-learning`.

Acceptance: tool call produces both a markdown file under skills/ AND a Tempyr journal entry; both reference each other.

Implementation note (2026-05-06): added the local Python meta tool in `maestro/src/jam_maestro/record_learning.py`. `RecordLearningRequest` writes a structured markdown skill note under the configured skills root (`JAM_SKILLS_ROOT`, first `[skills].folders` entry from `$JAM_HOME/config/skills.toml`, or the monorepo `skills/` fallback), including frontmatter for `scope`, `confidence`, `evidence`, `guidance`, and `originated-from-trace`. It then logs a paired Tempyr `decision` through the existing `TempyrJournalClient`; the journal entry tags `tool:record-learning`, `skill:<scope>`, and `trace:<originated-from-trace>`, and references the created skill file with `--file`. The skill note points back to the decision via those journal tags. `record-learning` is registered in `MaestroToolRegistry` as `meta.record-learning`.

Verification: `uv run pytest tests/unit/test_record_learning.py tests/unit/test_tool_registry.py`, `uv run pyright`, and `uv run ruff check`.
