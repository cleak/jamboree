---
id: principle-provider-agnostic-everywhere
type: constraint
status: active
created: 2026-05-04T03:23:48.535156907Z
updated: 2026-05-04T04:32:15.789618379Z
edges:
- target: comp-claude-code-adapter
  type: constrains
- target: comp-codex-cli-adapter
  type: constrains
- target: comp-litellm-backend
  type: constrains
- target: comp-maestro-process
  type: constrains
- target: comp-maestro-session-loop
  type: constrains
- target: comp-opencode-deepseek-adapter
  type: constrains
- target: comp-search-router
  type: constrains
- target: feat-deep-research
  type: constrains
- target: feat-maestro-orchestration-loop
  type: constrains
- target: feat-mcp-integration
  type: constrains
- target: feat-picker-layer-three-tier
  type: constrains
- target: feat-quota-tracking
  type: constrains
- target: feat-reviewer-adapters
  type: constrains
- target: feat-sandboxing-profile-x-backend
  type: constrains
- target: feat-search-router
  type: constrains
---
**§2.8 Provider-agnostic at every layer.**

Every external dependency on a specific provider — LLM model, search backend, sandbox backend, knowledge source — sits behind an abstraction allowing config-time swapping.

Canonical example: April 4 2026 Anthropic decision to block third-party harnesses from subscription use. The v3 design that quietly assumed Anthropic-hosted LLM and search became liabilities overnight. Lesson: never assume any provider's policy will hold.

LiteLLM for Maestro models. A search-router for search backends. A `SandboxBackend` trait for execution environments. A `HarnessAdapter` trait for Pickers.

*Why load-bearing:* the system runs for years; every provider's terms will change. The design expects this and makes it cheap.