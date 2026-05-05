---
scope: blueberry/runtime
---

# Blueberry — BRP Server Discipline

Blueberry has a localhost-only **Bevy Remote Protocol (BRP)** server for inspecting and controlling the running game from outside. Pickers use BRP as the **primary diagnostic** for any silent visual failure (zero geometry, invisible entity, wrong render pass) BEFORE changing code.

Source: `/home/caleb/blueberry/CLAUDE.md` (Agent Tooling section), `docs/agents/agent-tools.md`.

## Port discovery — read `.brp-port`

The BRP port is **derived from the worktree path** (range 15700–16699), so different Picker worktrees never collide. The port is written to `.brp-port` JSON at the worktree root once the server is ready:

```json
{"address": "127.0.0.1", "port": 15842, "pid": 12345}
```

Pickers read this file:
```bash
PORT=$(jq -r .port .brp-port)
PID=$(jq -r .pid .brp-port)
```

**Never hardcode 15702 in worktrees.** That's only the default for non-worktree runs. Use the file.

## Pre-flight checklist (mandatory before `cargo run`)

1. Check if `.brp-port` exists at worktree root.
2. If exists, read `pid` and check whether that process is alive (`kill -0 $PID 2>/dev/null`).
3. If PID is alive, HTTP-probe the BRP endpoint to confirm it's responsive.
4. **If alive AND responsive: REUSE the running instance.** Do not launch another.
5. If alive but unresponsive: terminate ONLY the process matching the PID in `.brp-port`. **Never terminate processes belonging to other agents or worktrees.** Verify the PID before killing.
6. If alive but unresponsive AND restart still fails: defer the task, record BRP failure in the session log, escalate via `notify-human`.
7. If PID is dead: file is stale; proceed with `cargo run` (the game overwrites `.brp-port` on startup).
8. If file does not exist: proceed with `cargo run` normally.

The game enforces a **claim guard**: if `.brp-port` exists with a live PID, startup logs an error and refuses to overwrite the file. A live PID does not guarantee a responsive endpoint — always verify with HTTP probe.

## Always launch via `cargo run`

Always start the app with `cargo run`, NOT by launching the exe directly. A direct-launched exe produces a headless process with no BRP listener.

## Method reference

Start every BRP-driven session with `blueberry.scene.describe` to discover entity IDs, then chain calls.

| Method | Purpose |
|---|---|
| `blueberry.scene.describe` | Entity IDs, transform/material summary — start here |
| `blueberry.scene.audit_transparency` | Live transparency audit; render route per entity |
| `blueberry.camera.get_pose` / `set_pose` | Camera framing |
| `blueberry.debug.get` / `set` | Render debug modes, physics debug, object selection |
| `blueberry.interaction.debug.get` | Interaction state snapshot (no log macros — use this) |
| `blueberry.material.patch` / `material.reset` | Temporary per-entity material changes |
| `blueberry.physics.raycast` | Physics probes with optional debug gizmos |
| `blueberry.script.enqueue` / `script.status` / `script.clear` | Gameplay automation |
| `blueberry.capture.request` / `capture.status` | Screenshot bundles |

For exact JSON-RPC payloads see `/home/caleb/blueberry/docs/agents/agent-tools.md`.

## Capturing a screenshot

```bash
PORT=$(jq -r .port .brp-port)
curl -s -X POST -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"blueberry.capture.request","params":{"basename":"my-shot"}}' \
  http://127.0.0.1:$PORT
# wait a few seconds, then check status
curl -s -X POST -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":2,"method":"blueberry.capture.status","params":{"id":1}}' \
  http://127.0.0.1:$PORT | jq .result.state
ls artifacts/agent-captures/my-shot/
```

## When to use BRP

- **Always** for visual failures before changing code. `blueberry.scene.describe` confirms entities exist; `blueberry.debug.get` inspects render debug state; `blueberry.scene.audit_transparency` reveals the live render route.
- For transparency-related issues specifically, audit_transparency shows whether a prop is on `CelOpaque`, `TransparentPrimitiveRoot`, or a stray `TransparentFaceMesh`.
- Use `block_input_capture: true` in `blueberry.debug.set` when mouse look or other input would interfere.
- Use `clear_selection: true` when changing selection-driven debug views.
- Prefer script `Capture` steps over manual `capture.request` + polling for looped automation.

## Multi-Picker isolation

Worktree-derived ports mean multiple Pickers in different worktrees can run the game simultaneously without conflict. Jamboree's worktree creation protocol (§6.9) ensures unique worktree paths, which produces unique BRP ports automatically. No orchestrator-side port allocation needed.

A Picker should never connect to a `.brp-port` outside its own worktree. If a Picker needs cross-worktree state (rare), escalate to the Manager.
