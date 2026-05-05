---
id: risk-mcp-ecosystem-volatility
type: risk
status: identified
created: 2026-05-04T03:47:12.656829293Z
updated: 2026-05-04T03:47:12.656829943Z
---
**§13.14 MCP ecosystem volatility.** MCP standard is young; servers may have spec drift, auth model changes, breaking versions.

Mitigation: per-project MCP config; Untrusted wrapping protects from prompt-injection; failed MCP calls are logged but non-fatal.