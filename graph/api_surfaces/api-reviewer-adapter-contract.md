---
id: api-reviewer-adapter-contract
type: api_surface
status: stable
created: 2026-05-04T03:53:34.097901316Z
updated: 2026-05-06T22:20:25Z
edges:
- target: comp-reviewer-adapter-trait
  type: exposed_by
- target: feat-reviewer-adapters
  type: exposed_by
---
Implemented in `crates/jam-tools-core/src/contracts.rs` as `jam_tools_core::contracts::ReviewerAdapter` (§4.7, §19.5):

```rust
pub trait ReviewerAdapter: Send + Sync {
    fn id(&self) -> ReviewerId;
    fn fetch_review(&self, pr: &PullRequestRef) -> ContractResult<Vec<ReviewArtifact>>;
    fn classify(&self, body: &Untrusted<String>) -> ArtifactKind;
    fn supports_reply(&self) -> bool;
    fn reply(&self, artifact: &ReviewArtifact, text: &str) -> ContractResult<()>;
}
```

Provider quirks absorbed by adapter rather than leaking into Maestro-facing tools.
