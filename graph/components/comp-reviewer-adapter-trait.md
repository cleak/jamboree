---
id: comp-reviewer-adapter-trait
type: component
status: active
created: 2026-05-04T03:34:46.799610254Z
updated: 2026-05-06T22:20:25Z
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
The shared trait for normalizing provider-specific review formats (§4.7, §19.5), now defined in `crates/jam-tools-core/src/contracts.rs`:

```rust
pub trait ReviewerAdapter: Send + Sync {
    fn id(&self) -> ReviewerId;
    fn fetch_review(&self, pr: &PullRequestRef) -> ContractResult<Vec<ReviewArtifact>>;
    fn classify(&self, body: &Untrusted<String>) -> ArtifactKind;
    fn supports_reply(&self) -> bool;
    fn reply(&self, artifact: &ReviewArtifact, text: &str) -> ContractResult<()>;
}
```

Provider quirks are absorbed by the adapter rather than leaking into Maestro-facing tools.
