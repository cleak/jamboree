---
id: principle-rust-trusted-core-python-agent-layer
type: constraint
status: active
created: 2026-05-04T03:23:48.962392283Z
updated: 2026-05-04T04:30:33.513175568Z
edges:
- target: comp-secret-string-newtype
  type: constrains
- target: comp-workspace-key-newtype
  type: constrains
- target: feat-maestro-orchestration-loop
  type: constrains
- target: feat-tech-stack-hardening
  type: constrains
- target: feat-tool-services-out-of-process
  type: constrains
- target: feat-ui-server
  type: constrains
---
**§2.11 Rust for the trusted core, Python for the agent layer.**

Rust for the substrate: tools, observation layer, NATS bus integration, sandboxing, journal store, UI server, patch agent. Python for the Maestro and the LLM-call path: better SDK ecosystem, better Pydantic-shaped tool I/O, faster iteration on prompts and skill logic.

The Python/Rust boundary is JSON schema with auto-generated Pydantic stubs (§11.2.6) — single source of truth for tool contracts.

*Why:* Rust where invariants matter (path safety, sandboxing, concurrency); Python where iteration matters (prompts, skills, LLM glue). The contract between them is types, not strings. Generated stubs catch contract drift at type-check time, not runtime.