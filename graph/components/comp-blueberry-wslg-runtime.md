---
id: comp-blueberry-wslg-runtime
type: component
status: active
created: 2026-05-04T05:54:27.502886617Z
updated: 2026-05-04T05:56:11.097734458Z
edges:
- target: feat-picker-layer-three-tier
  type: used_by
---
**WSLg runtime configuration for Blueberry** (per `docs/operations/wsl-runtime.md`).

The game runs from WSL2 Ubuntu via WSLg with software Vulkan (llvmpipe). Single-digit FPS by design — for screenshots, BRP smoke tests, visual diffs, NOT interactive play.

**Recommended invocation (for Pickers running the game):**
```bash
LD_LIBRARY_PATH=/usr/lib/wsl/lib \
WGPU_BACKEND=vulkan \
WGPU_FORCE_FALLBACK_ADAPTER=1 \
BLUEBERRY_WINDOW_RES=1920x1080 \
cargo run --release
```

`--release` is required (debug builds are too slow on software Vulkan).

**Performance reference (Ryzen 7 6800H / iGPU):**
- Playground @ 1080p: ~2.9 FPS (fillrate-sensitive)
- Yosemite clipmap @ 1080p: ~1.0 FPS (CPU-bound)
- Resolution barely affects clipmap scene — use 1080p for cleaner screenshots.

**Decision rule** (when to use WSLg lane):
- Visual validation of a render-pipeline change.
- Screenshot capture for diffs/docs/PR review.
- BRP-driven scripted smoke tests.

**Do NOT use WSLg lane to:**
- Tune game feel, animation timing, or anything where input latency or frame rate matters.
- Profile rendering performance — software Vulkan distorts cost shape.

For interactive frame rates, run on Windows natively (out of Jamboree's scope per `principle-linux-only-deployment`).

**Pickers spawned by Jamboree must propagate these env vars when running the game.** The harness adapter's spawn protocol should include the WSLg env block when `task_class` involves running the game.

**Known-bad alternatives (documented for skill files):**
- `WGPU_BACKEND=gl` does NOT work in WSLg (EGL only exposes swrast).
- Dozen (D3D12→Vulkan ICD) is non-conformant; visual artifacts in playground scenes.