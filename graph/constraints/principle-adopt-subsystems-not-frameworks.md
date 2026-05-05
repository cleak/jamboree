---
id: principle-adopt-subsystems-not-frameworks
type: constraint
status: active
created: 2026-05-04T03:23:48.663245064Z
updated: 2026-05-04T04:32:57.570050788Z
edges:
- target: comp-docker-sandbox-backend
  type: constrains
- target: comp-hermes-evolution-subsystem
  type: constrains
- target: comp-hermes-fts5-schema
  type: constrains
- target: feat-mcp-integration
  type: constrains
- target: feat-self-improvement
  type: constrains
- target: feat-skill-evolution-pipeline
  type: constrains
- target: feat-tempyr-knowledge-and-journal
  type: constrains
---
**§2.9 Adopt subsystems, not frameworks.**

A *subsystem* is a thing you can vendor or call as a library/process that doesn't drag opinions about architecture, scheduling, message flow, or knowledge ownership into your design. A *framework* takes over the top layer.

Concrete: Hermes' DSPy+GEPA optimization is a subsystem. Hermes' FTS5 schema is a subsystem. Hermes' Docker backend is a subsystem. Hermes Agent itself is a framework — adopting it would bring a Maestro loop, tool registry, gateway, scheduler, skill memory, messaging integrations all coupled to Hermes' worldview. We don't.

Tempyr is a subsystem too: we use its journal and graph but don't let Tempyr's worldview reshape our top-level architecture.

*Why:* a framework's update cycle becomes your update cycle; its bugs become your bugs; its design pivots become your design pivots. Subsystems give you optionality.