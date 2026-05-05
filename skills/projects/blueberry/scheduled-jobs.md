---
scope: blueberry/ops
---

# Blueberry — Scheduled Jobs (Ofelia)

Blueberry runs scheduled background jobs via Ofelia (Docker-based) for journal analysis, code audits, AI-powered reviews. Source: `/home/caleb/blueberry/SETUP.md`, `docs/operations/scheduled-jobs.md`, `ops/`.

<scope_for_jamboree>
**Ofelia is Blueberry's substrate, not Jamboree's.** Ofelia survives WSL2 idle shutdown because Docker Desktop runs as a Windows service. Jamboree's reconcilers (e.g. `comp-pr-status-poller`, `comp-trunk-fetcher`, `comp-skill-suspicion-reconciler`) run under `process-compose` as `maestro` — separate substrate.

Jamboree should **not** add jobs to Ofelia. Maestro reconcilers and the patch agent handle Jamboree's scheduled work.

Pickers working on Blueberry that need to interact with Ofelia jobs (read job logs, trigger a job manually) do so via Blueberry's `ops/run-job.sh` and `ops/setup.sh` patterns.
</scope_for_jamboree>

<existing_jobs>
| Job | Cadence | Purpose |
|---|---|---|
| `journal-publish` | Every 5 min | Runs `tempyr journal flush` to publish open sessions as git refs. |
| `journal-nightly` | Nightly 1 AM | Python pipeline: extraction, pattern analysis, recommendations from journal corpus. |
| `graph-broken-refs` | Nightly | Detects broken Tempyr edge references. |
| `stale-task-watchdog` | Nightly | Flags Tempyr task nodes that haven't progressed. |
| `daily-suggestions` | Nightly | AI-generated daily suggestions for project work. |

Schedules in `/home/caleb/blueberry/ops/scheduler/config.ini`. Logs in `artifacts/ops-logs/` (gitignored).
</existing_jobs>

<jamboree_meets_ofelia>
Two intersection points:

1. **`journal-publish` runs every 5 minutes** — same cadence as Maestro's periodic tick. Don't wake the Maestro on `journal-publish` events; it's noise. The orchestrator's own journal flush is independent.

2. **`stale-task-watchdog` may flag tasks the Maestro is actively working on**. If the Manager forwards a watchdog alert that mentions a `task_id` the Maestro currently has in flight, the Maestro's response is `notify-human(urgency=low, summary="task X still in flight, watchdog alert acknowledged")`. Don't try to suppress the watchdog from the Maestro side.
</jamboree_meets_ofelia>

<dispatching_pickers_for_ops_work>
If a task involves modifying Ofelia jobs or `ops/` config:
- `task_class`: `light-edit` (small Bash + Dockerfile changes usually).
- `harness`: any (this is shell/config work, not deep code).
- `initial_prompt`: reference Blueberry's `SETUP.md` Scheduled Jobs section explicitly.
- Acceptance includes restarting the scheduler (`cd ops && docker compose restart scheduler`).

Pickers MUST NOT add Ofelia jobs that would conflict with Jamboree's reconcilers (e.g. another `pr-status-poller`). When in doubt, escalate.
</dispatching_pickers_for_ops_work>
