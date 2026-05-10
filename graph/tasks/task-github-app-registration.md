---
id: task-github-app-registration
type: task
status: blocked
created: 2026-05-04T03:58:46.705875940Z
updated: 2026-05-06T19:03:50Z
edges:
- target: feat-reviewer-adapters
  type: child_of
---
Phase 2 (§12). GitHub App registration + installation token exchange via `octocrab`.

Per `comp-github-app-client`, `dec-github-app-not-pat`, `dec-etag-conditional-requests`.

Acceptance: `octocrab` exchanges App private key for installation token; token used for `git push` and PR comment APIs.

Implementation note (2026-05-06): `jam-svc-repo` now has the Octocrab App-token exchange path behind env / file / pass config. With `JAM_GITHUB_APP_ID`, `JAM_GITHUB_APP_INSTALLATION_ID`, and `JAM_GITHUB_APP_PRIVATE_KEY` / `JAM_GITHUB_APP_PRIVATE_KEY_FILE`, or `JAM_SECRETS_FILE` / maestro pass keys `jam/pickers/github-app-id`, `jam/pickers/github-app-installation-id`, and `jam/pickers/github-app-key`, it builds an Octocrab App client, calls `installation_and_token`, injects the short-lived installation token as `GH_TOKEN` for `gh pr create` and PR comment API fallback calls, and uses the token for `git push` through a non-interactive credential helper with `GIT_TERMINAL_PROMPT=0`. Mocked unit coverage verifies both the JWT-backed Octocrab exchange and token-backed push plumbing without real App credentials.

Doctor note (2026-05-06): `jam doctor` now has a real `github-app-key-valid` check. It accepts env config (`JAM_GITHUB_APP_ID`, `JAM_GITHUB_APP_INSTALLATION_ID`, and `JAM_GITHUB_APP_PRIVATE_KEY` / `JAM_GITHUB_APP_PRIVATE_KEY_FILE`) or maestro pass keys (`jam/pickers/github-app-id`, `jam/pickers/github-app-installation-id`, `jam/pickers/github-app-key`) and attempts the Octocrab installation-token exchange. Missing credentials warn; partial or invalid config fails loudly.

Blocked note (2026-05-06): the machine has a working `gh` PAT login for `cleak`, and the App-token exchange / push plumbing is now mock-tested, but the v5 acceptance requires real GitHub App auth. `jam doctor` now finds `jam/pickers/github-app-id` and `jam/pickers/github-app-key` in the maestro pass store, but `jam/pickers/github-app-installation-id` is missing, so `github-app-key-valid` fails loudly as partial config. To finish acceptance, seed the Blueberry App installation ID for the runtime user, rerun `jam doctor` until the Octocrab token-exchange check passes, then verify token-backed `git push` plus PR comment APIs against Blueberry.

External verification note (2026-05-06): using the stored maestro App ID/key to call GitHub's App API succeeded, but `/app/installations` returned zero installations. There is no installation ID to derive locally; the App must be installed on the Blueberry repository/account before this task can advance.
