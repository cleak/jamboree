---
id: risk-prompt-injection-secret-exfiltration
type: risk
status: identified
created: 2026-05-04T03:47:22.488831908Z
updated: 2026-05-04T03:47:22.488832401Z
---
**Threat #1 (high likelihood, security-setup §1).** Prompt injection driving secret exfiltration.

A CodeRabbit comment, MCP response, web-search result, or PR description contains text that induces the Picker LLM to do something it wasn't asked. "Exfiltrate creds" is a known attack pattern: read SSH keys, AWS credentials, browser session tokens, post somewhere.

Multi-user model (`feat-multi-user-security-model`) targets this directly — Pickers run as `picker` user with no read access to caleb's `.ssh`, `.gnupg`, `.password-store`. Plus `Untrusted<String>` discipline (`principle-untrusted-content-cannot-issue-commands`) and tool-shape invariants (no `merge-pr` etc.) prevent injection-driven actions.