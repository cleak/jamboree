---
scope: blueberry/reviewers/coderabbit
---

# Blueberry — CodeRabbit Conventions

Project-specific rules for handling CodeRabbit suggestions on Blueberry. Layered on top of the general `reviewers/coderabbit.md` skill.

<hot_path_extractions>
**Default: decline extraction suggestions on hot paths with rationale.**

Hot paths for Blueberry are listed in `projects/blueberry/hot-paths.md`. The `crates/blueberry-terrain/` family is the canonical hot zone — chunk generation, canyon construction, mesher loops.

Why: 3 PRs in early 2026 (#4421, #4502, #4513) had extraction suggestions accepted that introduced hot-path indirection costing 2-4% frame time. The pattern is consistent — extracted helpers force loads/calls that the inlined version avoided.

Reply template (use verbatim or close to it):
> Thanks for the suggestion. The canyon generator is on a hot path (per skills/projects/blueberry/hot-paths.md); we've measured 2-4% frame-time regressions from similar extractions in the past. Keeping the inlined version.
</hot_path_extractions>

<cold_path_extractions>
Accept extraction suggestions on cold paths by default. Cold paths include:
- Asset loading (`src/main.rs` setup, plugin registration).
- World generation initialization (one-shot, not per-frame).
- Editor / debug tooling under `src/agent_tools/`.
- Inventory authoring (`src/inventory_atelier.rs`).

When in doubt about whether a path is hot, run `cargo bench` before and after via a Picker; if no measurable regression, accept.
</cold_path_extractions>

<sdf_specific>
For SDF art assets (visible props/characters/cutters), CodeRabbit doesn't have the context for the project's cel/outline rendering policy. When a suggestion conflicts with `projects/blueberry/sdf-art-policy.md` (e.g. suggests adding micro-detail when silhouette readability is the priority), reply with rationale referencing the SDF skill.

Don't dispatch SDF Pickers via `task-types/coderabbit-review.md` — use the `sdf-modeling` skill in Blueberry's `.claude/skills/` directly when working on SDF art.
</sdf_specific>

<bevy_idioms>
CodeRabbit may suggest non-Bevy-0.18 idioms. Bevy's APIs change per minor; trust the Blueberry codebase over CodeRabbit when they conflict (per `projects/blueberry/code-conventions.md`).

Common false-positive patterns:
- Suggesting `lerp` where the codebase uses critically damped springs. **Always reject**; springs are the project default for any smoothed value.
- Suggesting `Visibility` toggles on shadow mesh entities. **Always reject**; `ObjectIdPlugin` overwrites them in `PostUpdate`.
- Suggesting direct `UiScale` patches. **Always reject**; use `BLUEBERRY_UI_SCALE` env var (centralized in `src/display.rs`).
</bevy_idioms>

<test_additions>
Accept test-addition suggestions by default. Tests are mandatory for new code (per `projects/blueberry/commit-validation.md`); regression tests for bug fixes are non-negotiable.

Dispatch via `task-types/coderabbit-review.md` with `task_class=light-edit` (test-only changes are usually small).
</test_additions>

<documentation_suggestions>
CodeRabbit often suggests adding docstrings or comments. Blueberry's convention (per `code-conventions.md`):
- **No low-value comments.** Comments explain "why", not "what".
- Self-documenting code through clear naming preferred.

If a suggestion adds "what" comments, reply with rationale citing `code-conventions.md`.
If a suggestion adds genuine "why" context (non-obvious invariant, gotcha, hidden constraint), accept.
</documentation_suggestions>

<related>
- `projects/blueberry/hot-paths.md` — canonical hot-path list.
- `projects/blueberry/code-conventions.md` — Blueberry coding style.
- `reviewers/coderabbit.md` — general CodeRabbit handling.
- `task-types/coderabbit-review.md` — dispatch pattern.
</related>
