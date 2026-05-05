---
scope: blueberry/journal
---

# Blueberry — Mandatory Journal Logging

Blueberry's existing convention: **every session that does real work MUST produce Tempyr journal entries.** A session with no journal entries is a failed session. Jamboree's Pickers inherit this discipline.

Source: `/home/caleb/blueberry/CLAUDE.md` (Knowledge Logging - MANDATORY section), `SETUP.md` (Session Journal section).

## Why

- The graph is curated knowledge; the journal is *how that knowledge was reached*.
- Failures are debuggable later because the trail exists.
- Skill evolution (DSPy + GEPA) trains on this corpus — sessions without entries are invisible to evolution.
- `journal_blame` and `journal_range` queries depend on the corpus existing.

## The eight kinds

Tempyr supports eight typed entry kinds. Use them; don't conflate.

| Kind | When |
|---|---|
| `plan` | What you're about to attempt and why. Log early, before edits. |
| `finding` | Something you learned by reading code or running a tool. |
| `assumption` | Something you're acting on without verifying (`polarity` required). |
| `question` | Something you don't know yet, to ask or look up. |
| `decision` | A choice with reasoning. Required: `chosen`, `rationale`, `reversible`, `detail` ≥ 50 chars. |
| `dead_end` | An approach that did not work. Required: `approach`, `failure_mode`, `detail` ≥ 50 chars. |
| `risk` | A potential problem identified but not yet hit. `severity` recommended. |
| `outcome` | Result of a plan. `passed` field; set `final = true` to close session. |

There is no `tool` kind. Log tool quirks as `finding --tag tool`.

## Minimum per session

**One `plan` early + one final `outcome --final`.** Most sessions also include findings and (when applicable) decisions or dead ends.

```bash
tempyr journal log --agent <agent> plan "Plan the specific work before editing files"

# ... work happens ...

tempyr journal log --agent <agent> finding "Found the journal publisher entrypoint still calling the legacy script"

tempyr journal log --agent <agent> decision "Use Tempyr journal refs for nightly input" \
  --chosen "Tempyr refs" \
  --rationale "The new publisher archives directly to refs/tempyr/journals/archive" \
  --reversible true \
  --detail "Nightly analysis should read the same archive namespace produced by tempyr journal flush so new sessions appear without a compatibility shim."

tempyr journal log --agent <agent> outcome "Completed the requested work and validation" \
  --passed true \
  --final
```

## Log as work happens, not batched

Log findings, decisions, and dead ends **as they happen**. Do not batch them at the end of the session — you'll forget detail or omit entries.

When a tool call fails unexpectedly, immediately:
```bash
tempyr journal log --agent <agent> dead_end "..." \
  --approach "..." \
  --failure-mode "..." \
  --detail "..."
```

If a skill influenced the failed approach, tag the entry `--tag "skill:<scope>"` so `skill-suspicion-reconciler` can pick it up.

## What's auto-emitted (do NOT double-log)

These transitions emit journal entries automatically:

| Trigger | Auto-emits |
|---|---|
| Task status change via Tempyr (`backlog`→`in_progress`, `in_progress`→`done`/`blocked`) | `plan`, `outcome`, or `risk` |
| Tempyr interview lifecycle events | provisional `plan`/`finding` and final `outcome` on commit |

If you change task status via `mcp__tempyr__graph_update_node`, the orchestrator's `task-lifecycle-handler` reconciler emits the corresponding journal entry. **Do not log the same transition manually.** It produces duplicates.

If you log it AND the auto-emit fires, prefer your manual entry (more detail) and accept the small duplication.

## Searching prior reasoning

Before re-deriving something, search the journal:

| Query | What it returns |
|---|---|
| `tempyr journal search "<query>"` | Hybrid retrieval (BM25 + vec0 + RRF + recency + kind boost) |
| `tempyr journal show <id>` | Fetch one entry |
| `tempyr journal range "<A..B>"` | Entries written while commits in range were checked out |
| `tempyr journal blame <file>` | Entries that referenced a path |

`journal_search` is your friend when planning a task — find prior dead_ends in the same area first.

## Lifecycle and diagnostics

`tempyr journal bootstrap --quiet` runs from session/worktree hooks to ensure directory layout exists.

`tempyr journal finalize --agent <agent>` marks the active session ready for publishing.

`tempyr journal flush` publishes ready sessions as git refs under `refs/tempyr/journals/archive/<YYYY>/<MM>/<DD>/<id>`. Blueberry's Ofelia `journal-publish` job runs flush every 5 minutes.

When sessions appear stuck or coverage is suspect, run:
```bash
tempyr doctor
tempyr journal status
tempyr journal logs --lines 50
tempyr journal lint
tempyr journal stats
```

## Discipline checklist

Before closing a Picker session, verify:
- [ ] At least one `plan` entry early in the session.
- [ ] Findings/decisions/dead_ends logged as they happened.
- [ ] Final `outcome --final` to close cleanly.
- [ ] No secrets, tokens, passwords, or large diffs in entry content.
- [ ] Tags include `skill:<scope>` on `dead_end` entries that involved a skill's guidance.

Auto-emitted entries from task status changes do NOT satisfy the per-session minimum — they're system events, not your reasoning trail.

## Trace propagation in entries

Every journal entry written via the orchestrator's wrapper auto-tags `trace:<id>` and `parent-trace:<id>` (per spec §23.3.4). Don't add them manually — the wrapper handles it.

If you're using the raw CLI (`tempyr journal log`) outside the orchestrator's wrapper, tags don't auto-attach. Pass `--tag trace:<current_trace_id>` explicitly if you want the entry to be reachable via `trace-replay`.
