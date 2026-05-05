---
id: comp-pass-secret-backend
type: component
status: planned
created: 2026-05-04T03:39:40.827518605Z
updated: 2026-05-04T05:02:16.999504762Z
edges:
- target: comp-jam-secrets
  type: depends_on
- target: comp-secret-string-newtype
  type: depends_on
- target: comp-seed-maestro-secrets-sh
  type: depended_on_by
- target: dec-pass-and-gpg-for-secrets
  type: has_decision
- target: feat-multi-user-security-model
  type: used_by
- target: feat-tech-stack-hardening
  type: used_by
- target: principle-linux-only-deployment
  type: constrained_by
---
`PassBackend` impl (§11.3). Wraps the standard Unix `pass` command.

Conventional naming under `pass` with prefix `jam/`:
```
jam/maestro/openai-api-key
jam/maestro/anthropic-api-key
jam/harness/claude-pro-token
jam/harness/codex-cli-token
jam/pickers/deepseek-api-key
jam/pickers/github-app-id
jam/pickers/github-app-key
jam/search/brave
jam/search/firecrawl
jam/search/exa
jam/search/linkup
jam/search/perplexity
jam/search/tavily
jam/mcp/composio
jam/notify/ntfy-token
jam/nats/token
jam/tailscale/auth-key
```

Under multi-user model (security-setup §5), the orchestrator's `pass` belongs to `maestro` user. Caleb's personal pass stays separate at `~caleb/.password-store/`.

Per memory: Maestro auth is ChatGPT subscription OAuth (no `pass` entry needed for `jam/maestro/openai-api-key` if subscription is the path). Other secrets — GitHub auth, search backends, ntfy, NATS — go in maestro's pass store.