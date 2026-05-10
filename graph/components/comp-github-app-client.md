---
id: comp-github-app-client
type: component
status: active
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

Implementation note (2026-05-06): the first App-token path now lives in `jam-svc-repo`. When App ID, installation ID, and private key are provided through env, `JAM_SECRETS_FILE`, or maestro pass, the service builds an Octocrab App client, exchanges the private-key JWT for an installation token with `installation_and_token`, injects the resulting short-lived token as `GH_TOKEN` for `gh pr create` / `gh api` fallback calls, and passes the token to `git push` through a constant non-interactive credential helper. Focused unit coverage uses a local mock GitHub API and a test RSA key to verify the Octocrab installation-token exchange and push credential plumbing without real credentials. Real acceptance still needs the actual App registration, installation, and secret seeding.

Doctor note (2026-05-06): `jam doctor` verifies the same App-token exchange when credentials are seeded. It reads env config or maestro pass keys `jam/pickers/github-app-id`, `jam/pickers/github-app-installation-id`, and `jam/pickers/github-app-key`, warning when the App has not been configured and failing on partial or invalid config.
