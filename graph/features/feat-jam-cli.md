---
id: feat-jam-cli
type: feature
status: active
created: 2026-05-04T03:28:26.759173906Z
updated: 2026-05-06T20:16:21Z
owner: caleb
edges:
- target: comp-jam-cli-binary
  type: uses
- target: comp-jam-setup
  type: uses
- target: dec-manual-project-onboarding-v1
  type: depends_on
- target: dec-single-project-per-instance
  type: depends_on
- target: jamboree-v5
  type: child_of
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
- target: task-cli-task-spawn-list-show
  type: parent_of
- target: task-document-blueberry-manual-onboarding
  type: parent_of
- target: task-jam-project-add-future
  type: parent_of
- target: task-jam-setup-and-jam-doctor-13-checks
  type: parent_of
- target: the-manager
  type: serves
---
The `jam` CLI binary (Rust, `crates/jam-cli/`). User-facing commands per §24.1, §11.4, §6.1, etc.:

- `jam setup` — preflight checks; refuses to install on bad environment.
- `jam doctor` — same checks any time, plus multi-user additions (security-setup §10).
- Future lifecycle wrapper: `jam start` / `jam stop` / `jam status`. Current operator path is root-launched `process-compose` because `process-compose.yaml` uses per-process `user: maestro`.
- `jam task spawn 'description' --project <p> --task-class <c> --priority <pri>` — opens a root trace and publishes `journal.task.requested`.
- `jam task list`, `jam task show <id>`, `jam task cleanup`.
- `jam trace replay <trace-id>` — calls into the trace-replay tool.
- `jam quota show`, `jam quota recalibrate`.
- `jam patch apply <service> <version>` — opens a patch trace, writes staged binary, emits `patch.staged`.
- `jam tempyr canonical-worktree recreate` — corruption recovery.
- `jam ui token` / `jam ui token --revoke <id>` / `jam ui token --revoke-all`.
- `jam pause-dispatch --reason <r>` / `jam resume-dispatch`.
- `jam maestro resume <session-id> --budget-extension N` / `jam maestro abandon <session-id>`.

Implementation note (2026-05-06): `jam quota show` is implemented against `tool.observe.query-quota` with traced NATS request-reply. It prints the full quota map or filters with `--harness-id`. `jam quota recalibrate` publishes traced `journal.quota.*` corrections for `available`, `exhausted`, and `low` states.
