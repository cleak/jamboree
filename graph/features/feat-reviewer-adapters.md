---
id: feat-reviewer-adapters
type: feature
status: draft
created: 2026-05-04T03:28:18.601560332Z
updated: 2026-05-04T04:39:08.022200270Z
owner: caleb
edges:
- target: api-reviewer-adapter-contract
  type: exposes
- target: comp-coderabbit-adapter
  type: uses
- target: comp-codex-review-adapter
  type: uses
- target: comp-github-app-client
  type: uses
- target: comp-pr-status-poller
  type: uses
- target: comp-reviewer-adapter-trait
  type: uses
- target: dec-etag-conditional-requests
  type: depends_on
- target: dec-github-app-not-pat
  type: depends_on
- target: jamboree-v5
  type: child_of
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-provider-agnostic-everywhere
  type: constrained_by
- target: principle-untrusted-content-cannot-issue-commands
  type: constrained_by
- target: task-coderabbit-reviewer-adapter
  type: parent_of
- target: task-codex-review-reviewer-adapter
  type: parent_of
- target: task-github-app-registration
  type: parent_of
- target: task-pr-status-poller-etag
  type: parent_of
---
CodeRabbit, codex-review, and custom-named-reviewer adapters normalize provider-specific review formats into the typed `ReviewArtifact` shape (§4.7).

Each implements:
```rust
pub trait ReviewerAdapter {
    fn id() -> ReviewerId;
    fn fetch_review(&self, pr: &PullRequestRef) -> Result<Vec<ReviewArtifact>>;
    fn classify(&self, body: &Untrusted<String>) -> ArtifactKind;
    fn supports_reply(&self) -> bool;
    fn reply(&self, artifact: &ReviewArtifact, text: &str) -> Result<()>;
}
```

`ReviewArtifact.body` is `Untrusted<String>` — never formatted into shell or system prompt without explicit handling per §11.2.4.

GitHub auth is **GitHub App with installation tokens** (15K/hour) and ETag-conditional polling (§4.7.1).