---
id: comp-reviewer-adapter-trait
type: component
status: planned
created: 2026-05-04T03:34:46.799610254Z
updated: 2026-05-04T05:00:09.576226996Z
edges:
- target: api-reviewer-adapter-contract
  type: exposes
- target: comp-coderabbit-adapter
  type: depended_on_by
- target: comp-codex-review-adapter
  type: depended_on_by
- target: feat-reviewer-adapters
  type: used_by
---
The trait for normalizing provider-specific review formats (§4.7, §19.5):

```rust
pub trait ReviewerAdapter: Send + Sync {
    fn id(&self) -> ReviewerId;
    fn fetch_review(&self, pr: &PullRequestRef) -> Result<Vec<ReviewArtifact>>;
    fn classify(&self, body: &Untrusted<String>) -> ArtifactKind;
    fn supports_reply(&self) -> bool;
    fn reply(&self, artifact: &ReviewArtifact, text: &str) -> Result<()>;
}
```

Provider quirks are absorbed by the adapter rather than leaking into Maestro-facing tools.