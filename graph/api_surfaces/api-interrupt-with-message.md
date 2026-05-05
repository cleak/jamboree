---
id: api-interrupt-with-message
type: api_surface
status: draft
created: 2026-05-04T03:53:05.708410286Z
updated: 2026-05-04T04:58:11.445161809Z
edges:
- target: comp-jam-svc-message
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
- target: feat-messaging-three-modes
  type: exposed_by
---
`interrupt-with-message(session-id, text, from?)` (§5.7).

Semantics: cancel the Picker's current turn at the next safe checkpoint and read this message. "Safe checkpoint" is between tool calls — current tool call finishes, next tool call doesn't start. Stops mid-LLM-stream cleanly.

Per-harness: cancellation key per harness (Esc for Claude Code, ^C-equivalent for Codex CLI, harness-specific protocol for OpenCode). After cancellation acknowledgement, message goes via stdin.

**Capability-gated**: only harnesses whose `capabilities().supports_interrupt == true`.

Confirmation lifecycle: `interrupt-requested` → `interrupt-accepted` → `delivered`. If `interrupt-accepted` doesn't arrive within `interrupt_timeout_secs` (default 30s), surface `interrupt-stuck` event.

UX intent: "I see what you're doing and I want to redirect you immediately, but I don't want you to lose mid-flight state."