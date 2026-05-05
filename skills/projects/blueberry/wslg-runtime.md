---
scope: blueberry/runtime
---

# Blueberry — WSLg Runtime

Blueberry runs on WSL2 Ubuntu with graphics displayed via WSLg (Windows). The game lane uses **software Vulkan (llvmpipe)** at single-digit FPS by design — for screenshots, BRP smoke tests, and visual diffs, **not** for interactive play, profiling, or game-feel tuning.

Source: `/home/caleb/blueberry/docs/operations/wsl-runtime.md`.

## Required env block

When dispatching a Picker that needs to run the game (`task_class` involves running, screenshotting, or BRP-driving the game), the harness adapter must propagate this env:

```bash
LD_LIBRARY_PATH=/usr/lib/wsl/lib \
WGPU_BACKEND=vulkan \
WGPU_FORCE_FALLBACK_ADAPTER=1 \
BLUEBERRY_WINDOW_RES=1920x1080 \
cargo run --release
```

What each setting does:
- `LD_LIBRARY_PATH=/usr/lib/wsl/lib` — makes WSLg-provided D3D12 mapping libraries discoverable. Required even on llvmpipe path.
- `WGPU_BACKEND=vulkan` — pins wgpu to Vulkan. GL backend uses EGL on Wayland in WSLg, where only `swrast` is exposed (wgpu finds no adapter).
- `WGPU_FORCE_FALLBACK_ADAPTER=1` — lets wgpu select `device_type: Cpu` (lavapipe/llvmpipe). Without it, Bevy panics with "Unable to find a GPU".
- `BLUEBERRY_WINDOW_RES=1920x1080` — sets primary window resolution. Default is 1280x720; 1080p gives cleaner screenshots.

`--release` is required — debug builds are too slow on software Vulkan.

Expected stderr on a working run:
```
AdapterInfo { name: "llvmpipe (...)", device_type: Cpu, driver: "llvmpipe", backend: Vulkan }
WARN The selected adapter is using a driver that only supports software rendering. ...
```
The B0006 software-renderer warning is expected on this lane.

## Performance reference

Measured on Ryzen 7 6800H / iGPU (release build):

| Scene | 640x360 | 1280x720 | 1920x1080 |
|---|---|---|---|
| playground | ~8.0 | ~5.0 | ~2.9 |
| Yosemite clipmap | ~1.2 | ~1.0 | ~1.0 |

The clipmap scene is geometry/CPU-bound — resolution barely moves the needle. Use 1080p for cleaner screenshots without meaningful cost.

## Decision rule

**Use the WSLg lane when** the task needs:
- Visual validation of a render-pipeline change end-to-end.
- Screenshot capture for diffs/docs/PR review.
- BRP-driven scripted smoke tests.

**Do NOT use the WSLg lane to:**
- Tune game feel, animation timing, or anything where input latency or frame rate matters.
- Profile rendering performance — software Vulkan distorts the cost shape.
- Stress-test interactive scenarios.

For interactive frame rates, run on Windows natively. That's outside Jamboree's scope (`principle-linux-only-deployment`); escalate to the Manager via `notify-human` if a task fundamentally requires interactive frame rates.

## Known-bad alternatives (do not try)

- **`WGPU_BACKEND=gl`** — does not work in WSLg (EGL only exposes swrast).
- **Dozen (D3D12→Vulkan ICD)** — non-conformant; visual artifacts in playground scenes; slower than llvmpipe on geometry-bound scenes.
- **Forcing winit to X11** — does not change which EGL display wgpu queries; same swrast result.

## Verifying WSLg is wired up

Before dispatching a Picker that runs the game, verify the env from a quick smoke test:

```bash
echo "DISPLAY=$DISPLAY"
echo "WAYLAND_DISPLAY=$WAYLAND_DISPLAY"
ls -l /dev/dxg
ls -ld /mnt/wslg /usr/lib/wsl/lib
```

`/dev/dxg`, `/mnt/wslg`, and `/usr/lib/wsl/lib` must all exist. If any missing, escalate to Manager — orchestrator can't fix WSLg setup automatically.

## Dev-library prerequisites

The Picker's worktree must have these Linux dev libs installed (one-time apt install on the host):
```
mesa-utils vulkan-tools jq
libwayland-dev libxkbcommon-dev libudev-dev libasound2-dev libx11-dev pkg-config
```

Without dev libs, `cargo build` fails on `wayland-client` / `xkbcommon` / `udev` lookups. This is a host-level prerequisite; Pickers don't install packages.

## Docker lane (deferred)

Blueberry has a documented Docker runtime lane (`--device=/dev/dxg`, mount `/usr/lib/wsl` and `/mnt/wslg`). Functionally works but offers no perf advantage over direct lane. Documented for future sandboxed automation; for now, Pickers use the direct lane.

When Jamboree's `feat-sandboxing-profile-x-backend` adds the Docker backend, the Docker-lane env vars and mounts become relevant. Until then, ignore.
