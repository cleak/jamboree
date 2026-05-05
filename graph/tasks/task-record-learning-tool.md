---
id: task-record-learning-tool
type: task
status: backlog
created: 2026-05-04T04:01:49.611225301Z
updated: 2026-05-04T04:01:49.611225858Z
---
Implement `record-learning(scope, evidence, guidance, counterexample, confidence, originated-from-trace?)`. Writes a structured skill note as markdown AND emits a Tempyr `decision`/`finding` entry tagged with the skill scope and the new skill's path.

Per `feat-record-learning`, `dec-record-learning-emits-dual`, `api-record-learning`.

Acceptance: tool call produces both a markdown file under skills/ AND a Tempyr journal entry; both reference each other.