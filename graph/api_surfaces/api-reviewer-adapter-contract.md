---
id: api-reviewer-adapter-contract
type: api_surface
status: draft
created: 2026-05-04T03:53:34.097901316Z
updated: 2026-05-04T05:00:09.576227306Z
edges:
- target: comp-reviewer-adapter-trait
  type: exposed_by
- target: feat-reviewer-adapters
  type: exposed_by
---
The `ReviewerAdapter` trait (§4.7, §19.5):

```rust
pub trait ReviewerAdapter {
    fn id(&self) -> ReviewerId;
    fn fetch_review(&self, pr: &PullRequestRef) -> Result<Vec<ReviewArtifact>>;
    fn classify(&self, body: &Untrusted<String>) -> ArtifactKind;
    fn supports_reply(&self) -> bool;
    fn reply(&self, artifact: &ReviewArtifact, text: &str) -> Result<()>;
}
```

Provider quirks absorbed by adapter rather than leaking into Maestro-facing tools.