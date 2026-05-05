---
id: comp-github-app-client
type: component
status: planned
created: 2026-05-04T03:34:49.376913558Z
updated: 2026-05-04T05:02:26.596191629Z
edges:
- target: comp-coderabbit-adapter
  type: depended_on_by
- target: comp-codex-review-adapter
  type: depended_on_by
- target: comp-jam-svc-session
  type: depended_on_by
- target: comp-pr-status-poller
  type: depended_on_by
- target: dec-github-app-not-pat
  type: has_decision
- target: feat-reviewer-adapters
  type: used_by
---
Shared GitHub API client used by reviewer adapters and `pr-status-poller` (§4.7.1).

**GitHub App authentication** with installation tokens (15,000/hour vs 5,000 for PAT). Setup is one-time: register the app, generate a private key, install on repos, store key in `pass`, exchange for installation tokens at startup. The `octocrab` crate handles the dance.

**ETag-based conditional requests** as defense-in-depth. Each PR poll caches the response ETag; subsequent polls send `If-None-Match` and get 304 (no rate limit consumed) when nothing changed. With ETag caching, ~70% of polls return 304 in steady state.

```rust
pub struct GitHubClient {
    app_id: u64,
    installation_id: u64,
    private_key: SecretString,
    etag_cache: Arc<Mutex<HashMap<EndpointKey, EtagEntry>>>,
}
```

**Picker secrets distribution**: Pickers don't get the App private key directly. The harness adapter exchanges App key → installation token → picker-scoped token before spawn. Token expires in 1 hour; refresh logic in the harness adapter reissues tokens for long-running Pickers via NATS callback.

Why GitHub App over PAT: 3x rate limit, per-installation rate limits, conditional requests count only for non-304 responses.