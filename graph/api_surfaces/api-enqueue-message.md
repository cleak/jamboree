---
id: api-enqueue-message
type: api_surface
status: draft
created: 2026-05-04T03:53:03.320642416Z
updated: 2026-05-04T04:58:02.184472918Z
edges:
- target: comp-jam-svc-message
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
- target: feat-messaging-three-modes
  type: exposed_by
---
`enqueue-message(session-id, text, from?)` (§5.7). **Default mode** — least disruptive.

Semantics: deliver this message at the next prompt boundary in the Picker's input loop. A prompt boundary is when the Picker has finished tool-call execution, finished streaming a model response, and is waiting for the next input.

Per-harness implementation: harness adapter writes to a per-session FIFO that the harness's stdin-handler reads when transitioning to prompt-waiting state.

Confirmation lifecycle: `queued` → `delivered` → (optional heuristic) `acknowledged`. Surfaced via `picker.<session-id>.msg.status` events.

UX intent: "btw I'd prefer rayon for this" / "the spec lives at docs/cstdc.md" / "skip the visualizer test."