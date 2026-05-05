---
id: task-trace-continuity-integration-test
type: task
status: backlog
created: 2026-05-04T04:01:18.562987499Z
updated: 2026-05-04T04:01:18.562988386Z
---
Integration test: a fixture spawns a fake task end-to-end (mock harness, mock LLM, mock GitHub), then asserts that all journal entries from spawn through merge share or descend from one root trace.

Per §23.6 *Layer 3*, `risk-trace-propagation-discipline-gaps`.

CI catches regressions.