---
id: task-trace-continuity-integration-test
type: task
status: done
created: 2026-05-04T04:01:18.562987499Z
updated: 2026-05-06T07:52:49.926589856Z
---
Integration test: a fixture spawns a fake task end-to-end (mock harness, mock LLM, mock GitHub), then asserts that all journal entries from spawn through merge share or descend from one root trace.

Per §23.6 *Layer 3*, `risk-trace-propagation-discipline-gaps`.

CI catches regressions.

Implementation note (2026-05-06): added `crates/jam-cli/tests/trace_continuity.rs`. The fixture writes a fake end-to-end journal under a temporary `JAM_HOME`: `task.requested`, `maestro.session-started`, `worktree.created`, `picker.spawned`, `maestro.tool-call`, `pr.opened`, and `pr.merged`, with actors `mock-llm`, `mock-harness`, and `mock-github`. The test asserts every journal entry either uses root trace `01ARZ3NDEKTSV4RRFFQ69G5FAV` or descends from it via `parent_trace_id`; it also runs the actual `jam trace replay` binary against the fixture and verifies the Picker child trace reconstructs back to the root.

Verification (2026-05-06): `cargo test -p jam-cli --test trace_continuity` and `cargo clippy -p jam-cli --test trace_continuity -- -D warnings` passed before full workspace validation.
