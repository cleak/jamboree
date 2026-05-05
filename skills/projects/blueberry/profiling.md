---
scope: blueberry/profiling
---

# Blueberry — Profiling

Performance profiling on Blueberry uses specific tools and conventions. Source: `/home/caleb/blueberry/docs/operations/profiling.md`.

<when_to_profile>
Profile when:
- A PR is suspected of regressing frame time (e.g. CodeRabbit accepted an extraction in hot-path code).
- Investigating user-reported performance issue.
- Validating a performance fix before merge.
- Establishing baseline for a new scene or major feature.

**Don't profile on the WSLg lane.** Software Vulkan distorts the cost shape; results are not representative. Per `projects/blueberry/wslg-runtime.md`, perf work needs Windows-native or real Linux GPU runs.

For Jamboree-scope work, **profiling Pickers must escalate to the Manager** if Windows-native is required. The Manager runs the profile and attaches the trace to the task.
</when_to_profile>

<profiling_scenarios>
Existing reproducible profiling scenarios in `/home/caleb/blueberry/docs/operations/`:
- `profiling-canyon-traverse.ron` — canyon scene + player traversal pattern.
- `profiling-transparent-panels.ron` — transparency render route exercise.

These are scripted via `blueberry.script.enqueue` (BRP) so the trace is reproducible. Pickers can dispatch these via BRP per `projects/blueberry/brp-server.md`.
</profiling_scenarios>

<measurement_method>
Bevy 0.18 frame-time measurement (per `projects/blueberry/wslg-runtime.md`):
- `Time<Virtual>` clamps at 250ms per frame.
- Three-sample steady-state averages — single-frame readings are too noisy.
- For samples near `last_delta=0.25`, FPS is `(virt_dt / 0.25) / wall_dt` — average over the sampling window, not single-frame.

When reporting to Manager, include:
- Mean FPS over a 5-10 sample window.
- Scene + resolution.
- Hardware (Ryzen 7 6800H / iGPU is the standard reference).
- Build flags (`--release`, `--no-default-features` if used).
</measurement_method>

<profile_then_act>
Workflow when investigating a regression:
1. Run baseline profile on the unmodified branch.
2. Apply the change.
3. Run the same scenario.
4. Compare. If >1% regression, the change is hot-path-significant.
5. If hot-path regression, decline the change or find an alternative implementation.
6. Document the result via `record-learning` if the pattern is generalizable; update `projects/blueberry/hot-paths.md`.
</profile_then_act>

<related>
- `projects/blueberry/hot-paths.md` — known hot paths (declines extraction by default).
- `projects/blueberry/wslg-runtime.md` — why WSLg lane is unsuitable for profiling.
- `projects/blueberry/brp-server.md` — script-driven scenario reproduction.
</related>
