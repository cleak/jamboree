## Summary

- Makes Codex Picker launches explicit about model and thinking depth instead of inheriting Codex CLI last-used defaults.
- Defaults `jam-svc-session` to `JAM_CODEX_MODEL=gpt-5.3-codex` and `JAM_CODEX_REASONING_EFFORT=high`, with request-level overrides still taking precedence.
- Updates Maestro dispatch to send task-class reasoning effort, including `high` for Jamboree self-modification tasks, and documents the runtime policy in Tempyr graph nodes.

## Verification

- `codex --version` -> `codex-cli 0.133.0`
- `codex exec --help` confirmed `--model` and `--config <key=value>` are supported.
- `rustfmt --edition 2024 --check crates/jam-svc-session/src/main.rs` -> passed.
- `cargo test -p jam-svc-session` -> passed, 44 tests.
- `python3 -m py_compile maestro/src/jam_maestro/dispatch.py maestro/tests/unit/test_dispatch.py` -> passed.
- `git diff --check` -> passed.

Could not run:

- `cargo fmt --check` because this environment has no `cargo fmt` subcommand installed.
- Python `pytest` / `ruff` gates because `pytest`, `ruff`, `uv`, and `pydantic` are not installed in this Picker environment.
- `tempyr validate` because the graph already has unrelated validation errors: `dec-post-picker-coordination` references missing `comp-jam-task-lifecycle` and is missing a reverse edge from `comp-jam-svc-session`.

## Build And Deploy

No live deploy was run.

To build:

```bash
cargo build --release -p jam-svc-session
```

To deploy the session service binary:

```bash
jam deploy session
```

To apply the `process-compose.yaml` environment changes after merge:

```bash
sudo -u maestro /opt/jam/bin/process-compose project update \
  -f /home/caleb/jamboree/process-compose.yaml \
  -u /home/maestro/.jam/process-compose.sock -U
sudo -u maestro /opt/jam/bin/process-compose process restart jam-svc-session \
  -u /home/maestro/.jam/process-compose.sock -U
```
