## Summary

Adds real-time, non-spending subscription harness probes to `jam-svc-observe` quota responses:

- Codex CLI is checked with `codex login status`.
- Claude Code is checked with `claude auth status --json`.
- Live probe status is exposed alongside journal/config-derived remaining quota, keeping remaining-headroom accounting conservative.
- API-budget quota rows continue to carry provider/model/spend data, so configured DeepSeek, OpenRouter, and other API work stays visible.

Repairs the dashboard by using `GET /api/quotas`, refreshing every 30 seconds, surfacing quota fetch errors on the Quotas page, and showing provider, budget, and live subscription status columns. `/api/quota` remains as a compatibility alias.

Recorded the non-obvious design choice in Tempyr journal entry `j-c9846c50b134423fa182df3d6773208b`: live auth/status probes are account freshness, not authoritative remaining subscription counters, because the first-party CLIs do not expose a stable no-spend remaining-quota endpoint.

## Verification

Passed:

```bash
npm --prefix ui ci
npm --prefix ui run build
rustfmt --edition 2021 crates/jam-svc-observe/src/main.rs crates/jam-ui-server/src/main.rs
cargo test -p jam-svc-observe -p jam-ui-server
cargo clippy -p jam-svc-observe --all-targets -- -D warnings
```

Partially blocked / known existing issues:

```bash
cargo fmt --check
```

`cargo fmt` is unavailable in this environment (`cargo` has no `fmt` subcommand), so the modified Rust files were formatted directly with `rustfmt`.

```bash
cargo clippy -p jam-svc-observe -p jam-ui-server --all-targets -- -D warnings
```

`jam-svc-observe` passes. The combined command still fails on pre-existing `jam-ui-server` strict clippy findings unrelated to this change, including `too_many_lines` in `deploy_handler` and existing test raw string hash warnings.

```bash
tempyr validate
```

Fails on pre-existing graph consistency errors in `dec-post-picker-coordination` (`comp-jam-task-lifecycle` target missing and a missing reverse edge to `comp-jam-svc-session`).

## Build And Deploy

No live deploy was run.

Build commands:

```bash
cargo build --release -p jam-svc-observe -p jam-ui-server
npm --prefix ui ci
npm --prefix ui run build
```

Deploy commands when approved:

```bash
jam deploy observe
jam deploy ui-server
```
